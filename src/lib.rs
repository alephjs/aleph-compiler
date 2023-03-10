mod css;
mod error;
mod hmr;
mod minifier;
mod resolve_fold;
mod resolver;
mod swc;
mod swc_helpers;

#[cfg(test)]
mod tests;

use minifier::MinifierOptions;
use resolver::{DependencyDescriptor, Resolver};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::{cell::RefCell, rc::Rc};
use swc::{EmitOptions, SWC};
use swc_ecmascript::ast::EsVersion;
use url::Url;
use wasm_bindgen::prelude::{wasm_bindgen, JsValue};

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Options {
  pub aleph_pkg_uri: Option<String>,
  pub lang: Option<String>,
  pub target: Option<String>,
  pub import_map: Option<String>,
  pub global_version: Option<String>,
  pub graph_versions: Option<HashMap<String, String>>,
  pub strip_data_export: Option<bool>,
  pub resolve_remote_module: Option<bool>,
  pub is_dev: Option<bool>,
  pub source_map: Option<bool>,
  pub jsx_pragma: Option<String>,
  pub jsx_pragma_frag: Option<String>,
  pub jsx_import_source: Option<String>,
  pub react_refresh: Option<bool>,
  pub minify: Option<MinifierOptions>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransformOutput {
  pub code: String,

  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub deps: Vec<DependencyDescriptor>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub map: Option<String>,
}

#[wasm_bindgen(js_name = "parseDeps")]
pub fn parse_deps(specifier: &str, code: &str, options: JsValue) -> Result<JsValue, JsValue> {
  console_error_panic_hook::set_once();

  let options: Options = serde_wasm_bindgen::from_value(options).unwrap();
  let importmap = import_map::parse_from_json(
    &Url::from_str("file:///").unwrap(),
    options.import_map.unwrap_or("{}".into()).as_str(),
  )
  .expect("could not pause the import map")
  .import_map;
  let resolver = Rc::new(RefCell::new(Resolver::new(
    specifier,
    "",
    importmap,
    HashMap::new(),
    None,
    false,
    false,
  )));
  let module = SWC::parse(specifier, code, EsVersion::Es2022, options.lang).expect("could not parse the module");
  let deps = module.parse_deps(resolver).expect("could not parse the module");

  Ok(serde_wasm_bindgen::to_value(&deps).unwrap())
}

#[wasm_bindgen(js_name = "transform")]
pub fn transform(specifier: &str, code: &str, options: JsValue) -> Result<JsValue, JsValue> {
  console_error_panic_hook::set_once();

  let options: Options = serde_wasm_bindgen::from_value(options).unwrap();
  let importmap = import_map::parse_from_json(
    &Url::from_str("file:///").unwrap(),
    options.import_map.unwrap_or("{}".into()).as_str(),
  )
  .expect("could not pause the import map")
  .import_map;
  let resolver = Rc::new(RefCell::new(Resolver::new(
    specifier,
    &options.aleph_pkg_uri.unwrap_or("https://deno.land/x/aleph".into()),
    importmap,
    options.graph_versions.unwrap_or_default(),
    options.global_version,
    options.resolve_remote_module.unwrap_or_default(),
    options.is_dev.unwrap_or_default(),
  )));
  let target = match options.target.unwrap_or_default().as_str() {
    "es2015" => EsVersion::Es2015,
    "es2016" => EsVersion::Es2016,
    "es2017" => EsVersion::Es2017,
    "es2018" => EsVersion::Es2018,
    "es2019" => EsVersion::Es2019,
    "es2020" => EsVersion::Es2020,
    "es2021" => EsVersion::Es2021,
    "es2022" => EsVersion::Es2022,
    _ => EsVersion::Es2022, // use latest version
  };
  let module = SWC::parse(specifier, code, target, options.lang).expect("could not parse the module");
  let (code, map) = module
    .transform(
      resolver.clone(),
      &EmitOptions {
        target,
        jsx_pragma: options.jsx_pragma,
        jsx_pragma_frag: options.jsx_pragma_frag,
        jsx_import_source: options.jsx_import_source,
        react_refresh: options.react_refresh.unwrap_or_default(),
        strip_data_export: options.strip_data_export.unwrap_or_default(),
        minify: options.minify,
        source_map: options.source_map.unwrap_or_default(),
      },
    )
    .expect("could not transform the module");
  let r = resolver.borrow();

  Ok(
    serde_wasm_bindgen::to_value(&TransformOutput {
      code,
      deps: r.deps.clone(),
      map,
    })
    .unwrap(),
  )
}

#[wasm_bindgen(js_name = "parcelCSS")]
pub fn parcel_css(filename: &str, code: &str, config_raw: JsValue) -> Result<JsValue, JsValue> {
  let config: css::Config = serde_wasm_bindgen::from_value(config_raw).unwrap();
  let res = css::compile(filename.into(), code, &config)?;
  Ok(serde_wasm_bindgen::to_value(&res).unwrap())
}
