use crate::resolver::Resolver;
use crate::swc_helpers::{is_call_expr_by_name, new_str};
use std::{cell::RefCell, rc::Rc};
use swc_common::{Span, DUMMY_SP};
use swc_ecmascript::ast::*;
use swc_ecmascript::visit::{noop_fold_type, Fold, FoldWith};

pub fn resolve_fold(
  resolver: Rc<RefCell<Resolver>>,
  strip_data_export: bool,
  mark_import_src_location: bool,
) -> impl Fold {
  ResolveFold {
    resolver,
    strip_data_export,
    mark_import_src_location,
  }
}

pub struct ResolveFold {
  resolver: Rc<RefCell<Resolver>>,
  strip_data_export: bool,
  mark_import_src_location: bool,
}

impl Fold for ResolveFold {
  noop_fold_type!();

  // fold&resolve import/export url
  fn fold_module_items(&mut self, module_items: Vec<ModuleItem>) -> Vec<ModuleItem> {
    let mut items = Vec::<ModuleItem>::new();

    for item in module_items {
      match item {
        ModuleItem::ModuleDecl(decl) => {
          let item: ModuleItem = match decl {
            // match: import React, { useState } from "https://esm.sh/react"
            ModuleDecl::Import(import_decl) => {
              if import_decl.type_only {
                // ingore type import
                ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl))
              } else {
                let mut resolver = self.resolver.borrow_mut();
                let resolved_url = resolver.resolve(
                  import_decl.src.value.as_ref(),
                  false,
                  mark_span(&import_decl.src.span, self.mark_import_src_location),
                );
                ModuleItem::ModuleDecl(ModuleDecl::Import(ImportDecl {
                  src: Box::new(new_str(&resolved_url)),
                  ..import_decl
                }))
              }
            }
            // match: export { default as React, useState } from "https://esm.sh/react"
            // match: export * as React from "https://esm.sh/react"
            ModuleDecl::ExportNamed(NamedExport {
              type_only,
              specifiers,
              src: Some(src),
              span,
              asserts,
            }) => {
              if type_only {
                // ingore type export
                ModuleItem::ModuleDecl(ModuleDecl::ExportNamed(NamedExport {
                  span,
                  specifiers,
                  src: Some(src),
                  type_only,
                  asserts,
                }))
              } else {
                let mut resolver = self.resolver.borrow_mut();
                let resolved_url = resolver.resolve(
                  src.value.as_ref(),
                  false,
                  mark_span(&src.span, self.mark_import_src_location),
                );
                ModuleItem::ModuleDecl(ModuleDecl::ExportNamed(NamedExport {
                  span,
                  specifiers,
                  src: Some(Box::new(new_str(&resolved_url))),
                  type_only,
                  asserts,
                }))
              }
            }
            // match: export * from "https://esm.sh/react"
            ModuleDecl::ExportAll(ExportAll {
              src,
              span,
              asserts,
              type_only,
            }) => {
              let mut resolver = self.resolver.borrow_mut();
              let resolved_url = resolver.resolve(
                src.value.as_ref(),
                false,
                mark_span(&src.span, self.mark_import_src_location),
              );
              ModuleItem::ModuleDecl(ModuleDecl::ExportAll(ExportAll {
                span,
                src: Box::new(new_str(&resolved_url)),
                asserts,
                type_only,
              }))
            }
            // match: export const data = { ... }
            // match: export const muation = { ... }
            ModuleDecl::ExportDecl(ExportDecl {
              decl: Decl::Var(var),
              span,
            }) => {
              let mut data_export_idx = -1;
              let mut data_export_name = "".to_string();
              if self.strip_data_export && var.decls.len() > 0 {
                let mut i = 0;
                for decl in &var.decls {
                  if let Pat::Ident(bi) = &decl.name {
                    if decl.init.is_some() {
                      match bi.id.sym.as_ref() {
                        "data" | "mutation" | "GET" | "POST" | "PUT" | "PATCH" | "DELETE" => {
                          data_export_idx = i;
                          data_export_name = bi.id.sym.to_string();
                          break;
                        }
                        _ => {}
                      }
                    }
                  }
                  i += 1;
                }
              }
              if data_export_idx != -1 {
                let mut i = -1;
                let var = var.as_ref();
                ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(ExportDecl {
                  span,
                  decl: Decl::Var(Box::new(VarDecl {
                    decls: var
                      .decls
                      .clone()
                      .into_iter()
                      .map(|decl| {
                        i += 1;
                        if data_export_idx == i {
                          let mut init = Some(Box::new(Expr::Lit(Lit::Bool(Bool {
                            span: DUMMY_SP,
                            value: true,
                          }))));
                          if data_export_name == "data" || data_export_name == "mutation" {
                            if let Some(expr) = decl.init {
                              if let Expr::Object(obj) = *expr {
                                init = Some(Box::new(Expr::Object(ObjectLit {
                                  span: obj.span,
                                  props: obj
                                    .props
                                    .into_iter()
                                    .map(|prop| match prop {
                                      PropOrSpread::Prop(prop) => match *prop {
                                        Prop::Shorthand(ident) => {
                                          PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                                            key: PropName::Ident(ident),
                                            value: Box::new(Expr::Lit(Lit::Bool(Bool {
                                              span: DUMMY_SP,
                                              value: true,
                                            }))),
                                          })))
                                        }
                                        Prop::KeyValue(KeyValueProp { key, value, .. }) => {
                                          // if value is a boolean, we don't need to wrap it
                                          if let Expr::Lit(Lit::Bool(_)) = *value {
                                            PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp { key, value })))
                                          } else {
                                            PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                                              key,
                                              value: Box::new(Expr::Lit(Lit::Bool(Bool {
                                                span: DUMMY_SP,
                                                value: true,
                                              }))),
                                            })))
                                          }
                                        }
                                        Prop::Method(MethodProp { key, .. }) => {
                                          PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                                            key,
                                            value: Box::new(Expr::Lit(Lit::Bool(Bool {
                                              span: DUMMY_SP,
                                              value: true,
                                            }))),
                                          })))
                                        }
                                        _ => PropOrSpread::Prop(prop),
                                      },
                                      _ => prop,
                                    })
                                    .collect(),
                                })))
                              }
                            }
                          }
                          VarDeclarator {
                            span: DUMMY_SP,
                            init,
                            ..decl
                          }
                        } else {
                          decl
                        }
                      })
                      .collect(),
                    span: DUMMY_SP,
                    kind: var.kind,
                    declare: var.declare,
                  })),
                }))
              } else {
                ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(ExportDecl {
                  span,
                  decl: Decl::Var(var),
                }))
              }
            }
            // match: export function data/mutation/GET/POST/PUT/PATCH/DELETE { ... }
            ModuleDecl::ExportDecl(ExportDecl {
              decl: Decl::Fn(decl),
              span,
            }) => {
              let is_api_method = match decl.ident.sym.as_ref() {
                "data" | "mutation" | "GET" | "POST" | "PUT" | "PATCH" | "DELETE" => true,
                _ => false,
              };
              if self.strip_data_export && is_api_method {
                ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(ExportDecl {
                  span: DUMMY_SP,
                  decl: Decl::Fn(FnDecl {
                    ident: decl.ident.clone(),
                    declare: decl.declare,
                    function: Box::new(Function {
                      span: DUMMY_SP,
                      params: vec![],
                      decorators: vec![],
                      // empty body
                      body: Some(BlockStmt {
                        span: DUMMY_SP,
                        stmts: vec![],
                      }),
                      is_generator: false,
                      is_async: false,
                      type_params: None,
                      return_type: None,
                    }),
                  }),
                }))
              } else {
                ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(ExportDecl {
                  span,
                  decl: Decl::Fn(decl),
                }))
              }
            }
            _ => ModuleItem::ModuleDecl(decl),
          };
          items.push(item.fold_children_with(self));
        }
        _ => {
          items.push(item.fold_children_with(self));
        }
      };
    }

    items
  }

  // resolve worker import url
  fn fold_new_expr(&mut self, mut new_expr: NewExpr) -> NewExpr {
    let ok = match new_expr.callee.as_ref() {
      Expr::Ident(id) => id.sym.as_ref().eq("Worker"),
      _ => false,
    };
    if ok {
      if let Some(args) = &mut new_expr.args {
        let src = match args.first() {
          Some(ExprOrSpread { expr, .. }) => match expr.as_ref() {
            Expr::Lit(lit) => match lit {
              Lit::Str(s) => Some(s),
              _ => None,
            },
            _ => None,
          },
          _ => None,
        };
        if let Some(src) = src {
          let mut resolver = self.resolver.borrow_mut();
          let new_src = resolver.resolve(
            src.value.as_ref(),
            true,
            mark_span(&src.span, self.mark_import_src_location),
          );

          args[0] = ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Lit(Lit::Str(new_str(&new_src)))),
          }
        }
      }
    };

    new_expr.fold_children_with(self)
  }

  // resolve dynamic import url
  fn fold_call_expr(&mut self, mut call: CallExpr) -> CallExpr {
    if is_call_expr_by_name(&call, "import") {
      let src = match call.args.first() {
        Some(ExprOrSpread { expr, .. }) => match expr.as_ref() {
          Expr::Lit(lit) => match lit {
            Lit::Str(s) => Some(s),
            _ => None,
          },
          _ => None,
        },
        _ => None,
      };
      if let Some(src) = src {
        let mut resolver = self.resolver.borrow_mut();
        let new_src = resolver.resolve(
          src.value.as_ref(),
          true,
          mark_span(&src.span, self.mark_import_src_location),
        );

        call.args[0] = ExprOrSpread {
          spread: None,
          expr: Box::new(Expr::Lit(Lit::Str(new_str(&new_src)))),
        }
      }
    }

    call.fold_children_with(self)
  }
}

fn mark_span(span: &Span, ok: bool) -> Option<Span> {
  if ok {
    Some(span.clone())
  } else {
    None
  }
}
