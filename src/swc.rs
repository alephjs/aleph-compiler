use crate::error::{DiagnosticBuffer, ErrorBuffer};
use crate::hmr::hmr;
use crate::minifier::{MinifierOptions, MinifierPass};
use crate::resolve_fold::resolve_fold;
use crate::resolver::{DependencyDescriptor, Resolver};

use std::{cell::RefCell, path::Path, rc::Rc};
use swc_common::comments::SingleThreadedComments;
use swc_common::errors::{Handler, HandlerFlags};
use swc_common::{chain, FileName, Globals, Mark, SourceMap};
use swc_ecma_transforms::optimization::simplify::dce;
use swc_ecma_transforms::pass::Optional;
use swc_ecma_transforms::proposals::decorators;
use swc_ecma_transforms::typescript::strip;
use swc_ecma_transforms::{compat, fixer, helpers, hygiene, react, Assumptions};
use swc_ecmascript::ast::{EsVersion, Module, Program};
use swc_ecmascript::codegen::text_writer::JsWriter;
use swc_ecmascript::codegen::Node;
use swc_ecmascript::parser::lexer::Lexer;
use swc_ecmascript::parser::{EsConfig, StringInput, Syntax, TsConfig};
use swc_ecmascript::visit::{as_folder, Fold, FoldWith};

/// Options for transpiling a module.
#[derive(Debug, Clone)]
pub struct EmitOptions {
  pub target: EsVersion,
  pub jsx_pragma: Option<String>,
  pub jsx_pragma_frag: Option<String>,
  pub jsx_import_source: Option<String>,
  pub react_refresh: bool,
  pub strip_data_export: bool,
  pub minify: Option<MinifierOptions>,
  pub source_map: bool,
}

impl Default for EmitOptions {
  fn default() -> Self {
    EmitOptions {
      target: EsVersion::Es2022,
      jsx_pragma: None,
      jsx_pragma_frag: None,
      jsx_import_source: None,
      react_refresh: false,
      strip_data_export: false,
      minify: None,
      source_map: false,
    }
  }
}

#[derive(Clone)]
pub struct SWC {
  pub specifier: String,
  pub module: Module,
  pub source_map: Rc<SourceMap>,
  pub comments: SingleThreadedComments,
}

impl SWC {
  /// parse source code.
  pub fn parse(specifier: &str, source: &str, target: EsVersion, lang: Option<String>) -> Result<Self, anyhow::Error> {
    print!("--- {} {:?} {:?}\n", specifier, target, lang);
    let source_map = SourceMap::default();
    let source_file = source_map.new_source_file(FileName::Real(Path::new(specifier).to_path_buf()), source.into());
    let sm = &source_map;
    let error_buffer = ErrorBuffer::new(specifier);
    let syntax = get_syntax(specifier, lang);
    let input = StringInput::from(&*source_file);
    let comments = SingleThreadedComments::default();
    let lexer = Lexer::new(syntax, target, input, Some(&comments));
    let mut parser = swc_ecmascript::parser::Parser::new_from(lexer);
    let handler = Handler::with_emitter_and_flags(
      Box::new(error_buffer.clone()),
      HandlerFlags {
        can_emit_warnings: true,
        dont_buffer_diagnostics: true,
        ..HandlerFlags::default()
      },
    );
    let module = parser
      .parse_module()
      .map_err(move |err| {
        let mut diagnostic = err.into_diagnostic(&handler);
        diagnostic.emit();
        DiagnosticBuffer::from_error_buffer(error_buffer, |span| sm.lookup_char_pos(span.lo))
      })
      .unwrap();

    Ok(SWC {
      specifier: specifier.into(),
      module,
      source_map: Rc::new(source_map),
      comments,
    })
  }

  /// parse deps in the module.
  pub fn parse_deps(&self, resolver: Rc<RefCell<Resolver>>) -> Result<Vec<DependencyDescriptor>, anyhow::Error> {
    let program = Program::Module(self.module.clone());
    let mut resolve_fold = resolve_fold(resolver.clone(), false, true);
    program.fold_with(&mut resolve_fold);
    let resolver = resolver.borrow();
    Ok(resolver.deps.clone())
  }

  /// transform a JS/TS/JSX/TSX file into a JS file, based on the supplied options.
  pub fn transform(
    self,
    resolver: Rc<RefCell<Resolver>>,
    options: &EmitOptions,
  ) -> Result<(String, Option<String>), anyhow::Error> {
    swc_common::GLOBALS.set(&Globals::new(), || {
      let unresolved_mark = Mark::new();
      let top_level_mark = Mark::fresh(Mark::root());
      let specifier_is_remote = resolver.borrow().specifier_is_remote;
      let is_dev = resolver.borrow().is_dev;
      let is_ts =
        self.specifier.ends_with(".ts") || self.specifier.ends_with(".mts") || self.specifier.ends_with(".tsx");
      let is_jsx = self.specifier.ends_with(".tsx") || self.specifier.ends_with(".jsx");
      let react_options = if let Some(jsx_import_source) = &options.jsx_import_source {
        let mut resolver = resolver.borrow_mut();
        let runtime = if is_dev { "/jsx-dev-runtime" } else { "/jsx-runtime" };
        let import_source = resolver.resolve(&(jsx_import_source.to_owned() + runtime), false, None);
        let import_source = import_source
          .split("?")
          .next()
          .unwrap_or(&import_source)
          .strip_suffix(runtime)
          .unwrap_or(&import_source)
          .to_string();
        if !is_jsx {
          resolver.deps.pop();
        }
        react::Options {
          runtime: Some(react::Runtime::Automatic),
          import_source: Some(import_source),
          ..Default::default()
        }
      } else {
        react::Options {
          pragma: options.jsx_pragma.clone(),
          pragma_frag: options.jsx_pragma_frag.clone(),
          ..Default::default()
        }
      };
      let assumptions = Assumptions::all();
      let passes = chain!(
        swc_ecma_transforms::resolver(unresolved_mark, top_level_mark, is_ts),
        Optional::new(react::jsx_src(is_dev, self.source_map.clone()), is_jsx),
        resolve_fold(resolver.clone(), options.strip_data_export, false),
        decorators::decorators(decorators::Config {
          legacy: true,
          emit_metadata: false,
          use_define_for_class_fields: false,
        }),
        Optional::new(
          compat::es2022::es2022(
            Some(&self.comments),
            compat::es2022::Config {
              class_properties: compat::es2022::class_properties::Config {
                private_as_properties: assumptions.private_fields_as_properties,
                constant_super: assumptions.constant_super,
                set_public_fields: assumptions.set_public_class_fields,
                no_document_all: assumptions.no_document_all
              }
            }
          ),
          should_enable(options.target, EsVersion::Es2022)
        ),
        Optional::new(
          compat::es2021::es2021(),
          should_enable(options.target, EsVersion::Es2021)
        ),
        Optional::new(
          compat::es2020::es2020(compat::es2020::Config {
            nullish_coalescing: compat::es2020::nullish_coalescing::Config {
              no_document_all: assumptions.no_document_all
            },
            optional_chaining: compat::es2020::opt_chaining::Config {
              no_document_all: assumptions.no_document_all,
              pure_getter: assumptions.pure_getters
            }
          }),
          should_enable(options.target, EsVersion::Es2020)
        ),
        Optional::new(
          compat::es2019::es2019(),
          should_enable(options.target, EsVersion::Es2019)
        ),
        Optional::new(
          compat::es2018(compat::es2018::Config {
            object_rest_spread: compat::es2018::object_rest_spread::Config {
              no_symbol: assumptions.object_rest_no_symbols,
              set_property: assumptions.set_spread_properties,
              pure_getters: assumptions.pure_getters,
            }
          }),
          should_enable(options.target, EsVersion::Es2018)
        ),
        Optional::new(
          compat::es2017(
            compat::es2017::Config {
              async_to_generator: compat::es2017::async_to_generator::Config {
                ignore_function_name: assumptions.ignore_function_name,
                ignore_function_length: assumptions.ignore_function_length
              }
            },
            Some(&self.comments),
            unresolved_mark,
          ),
          should_enable(options.target, EsVersion::Es2017)
        ),
        Optional::new(compat::es2016(), should_enable(options.target, EsVersion::Es2016)),
        compat::reserved_words::reserved_words(),
        helpers::inject_helpers(),
        Optional::new(
          strip::strip_with_config(strip_config_from_emit_options(), top_level_mark),
          !is_jsx
        ),
        Optional::new(
          strip::strip_with_jsx(
            self.source_map.clone(),
            strip_config_from_emit_options(),
            &self.comments,
            top_level_mark
          ),
          is_jsx
        ),
        Optional::new(
          react::refresh(
            is_dev,
            Some(react::RefreshOptions {
              refresh_reg: "$RefreshReg$".into(),
              refresh_sig: "$RefreshSig$".into(),
              emit_full_signatures: false,
            }),
            self.source_map.clone(),
            Some(&self.comments),
            top_level_mark
          ),
          options.react_refresh && !specifier_is_remote
        ),
        Optional::new(
          react::jsx(
            self.source_map.clone(),
            Some(&self.comments),
            react::Options {
              use_builtins: Some(true),
              development: Some(is_dev),
              ..react_options
            },
            top_level_mark
          ),
          is_jsx
        ),
        Optional::new(hmr(resolver.clone()), is_dev && !specifier_is_remote),
        dce::dce(
          dce::Config {
            module_mark: None,
            top_level: true,
            top_retain: vec![],
          },
          unresolved_mark
        ),
        Optional::new(
          as_folder(MinifierPass {
            cm: self.source_map.clone(),
            comments: Some(self.comments.clone()),
            unresolved_mark,
            top_level_mark,
            options: options.minify.unwrap_or(MinifierOptions { compress: false }),
          }),
          options.minify.is_some()
        ),
        hygiene(),
        fixer(Some(&self.comments)),
      );

      let (mut code, map) = self.emit(passes, options).unwrap();

      // remove dead deps by tree-shaking
      if options.strip_data_export {
        let mut resolver = resolver.borrow_mut();
        let mut deps: Vec<DependencyDescriptor> = Vec::new();
        let a = code.split("\"").collect::<Vec<&str>>();
        for dep in resolver.deps.clone() {
          if dep.specifier.ends_with("/jsx-runtime")
            || dep.specifier.ends_with("/jsx-dev-runtime")
            || a.contains(&dep.import_url.as_str())
          {
            deps.push(dep);
          }
        }
        resolver.deps = deps;
      }

      // resolve jsx-runtime url
      let mut jsx_runtime = None;
      let resolver = resolver.borrow();
      for dep in &resolver.deps {
        if dep.specifier.ends_with("/jsx-runtime") || dep.specifier.ends_with("/jsx-dev-runtime") {
          jsx_runtime = Some((dep.specifier.clone(), dep.import_url.clone()));
          break;
        }
      }
      if let Some((jsx_runtime, import_url)) = jsx_runtime {
        code = code.replace(
          format!("\"{}\"", jsx_runtime).as_str(),
          format!("\"{}\"", import_url).as_str(),
        );
      }

      Ok((code, map))
    })
  }

  /// Apply transform with the fold.
  pub fn emit<T: Fold>(&self, mut fold: T, options: &EmitOptions) -> Result<(String, Option<String>), anyhow::Error> {
    let program = Program::Module(self.module.clone());
    let program = helpers::HELPERS.set(&helpers::Helpers::new(false), || program.fold_with(&mut fold));
    let mut buf = Vec::new();
    let mut src_map_buf = Vec::new();
    let src_map = if options.source_map {
      Some(&mut src_map_buf)
    } else {
      None
    };

    {
      let writer = Box::new(JsWriter::new(self.source_map.clone(), "\n", &mut buf, src_map));
      let mut emitter = swc_ecmascript::codegen::Emitter {
        cfg: swc_ecmascript::codegen::Config {
          target: options.target,
          minify: options.minify.is_some(),
          ..Default::default()
        },
        comments: Some(&self.comments),
        cm: self.source_map.clone(),
        wr: writer,
      };
      program.emit_with(&mut emitter).unwrap();
    }

    // output
    let src = String::from_utf8(buf).unwrap();
    if options.source_map {
      let mut buf = Vec::new();
      self
        .source_map
        .build_source_map_from(&mut src_map_buf, None)
        .to_writer(&mut buf)
        .unwrap();
      Ok((src, Some(String::from_utf8(buf).unwrap())))
    } else {
      Ok((src, None))
    }
  }
}

fn get_es_config(jsx: bool) -> EsConfig {
  EsConfig {
    fn_bind: true,
    export_default_from: true,
    import_assertions: true,
    private_in_object: true,
    allow_super_outside_method: true,
    jsx,
    ..EsConfig::default()
  }
}

fn get_ts_config(tsx: bool) -> TsConfig {
  TsConfig {
    decorators: true,
    tsx,
    ..TsConfig::default()
  }
}

fn get_syntax(specifier: &str, lang: Option<String>) -> Syntax {
  let lang = if let Some(lang) = lang {
    lang
  } else {
    specifier
      .split(|c| c == '?' || c == '#')
      .next()
      .unwrap()
      .split('.')
      .last()
      .unwrap_or("js")
      .to_lowercase()
  };
  match lang.as_str() {
    "js" | "mjs" => Syntax::Es(get_es_config(false)),
    "jsx" => Syntax::Es(get_es_config(true)),
    "ts" | "mts" => Syntax::Typescript(get_ts_config(false)),
    "tsx" => Syntax::Typescript(get_ts_config(true)),
    _ => Syntax::Es(get_es_config(false)),
  }
}

fn strip_config_from_emit_options() -> strip::Config {
  strip::Config {
    import_not_used_as_values: strip::ImportsNotUsedAsValues::Remove,
    use_define_for_class_fields: true,
    no_empty_export: true,
    ..Default::default()
  }
}

fn should_enable(target: EsVersion, feature: EsVersion) -> bool {
  target < feature
}
