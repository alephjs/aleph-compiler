use import_map::ImportMap;
use path_slash::PathBufExt;
use pathdiff::diff_paths;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use swc_common::Span;
use url::Url;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyDescriptor {
  pub specifier: String,
  pub import_url: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub loc: Option<Span>,
  #[serde(skip_serializing_if = "is_false")]
  pub dynamic: bool,
}

/// A Resolver to resolve esm import/export URL.
pub struct Resolver {
  /// aleph pkg uri
  pub aleph_pkg_uri: String,
  /// the text specifier associated with the import/export statement.
  pub specifier: String,
  /// a flag indicating if the specifier is a remote(http) url.
  pub specifier_is_remote: bool,
  /// a ordered dependencies of the module
  pub deps: Vec<DependencyDescriptor>,
  /// development mode
  pub is_dev: bool,
  /// the global version
  pub global_version: Option<String>,
  /// the graph versions
  pub graph_versions: HashMap<String, String>,
  /// should resolve remote deps
  pub resolve_remote_deps: bool,
  // import maps
  import_map: ImportMap,
}

impl Resolver {
  pub fn new(
    specifier: &str,
    aleph_pkg_uri: &str,
    import_map: ImportMap,
    graph_versions: HashMap<String, String>,
    global_version: Option<String>,
    resolve_remote_deps: bool,
    is_dev: bool,
  ) -> Self {
    Resolver {
      aleph_pkg_uri: aleph_pkg_uri.into(),
      specifier: specifier.into(),
      specifier_is_remote: is_http_url(specifier),
      deps: Vec::new(),
      import_map,
      graph_versions,
      global_version,
      is_dev,
      resolve_remote_deps,
    }
  }

  /// fix remote url for dev mode.
  //  - `https://esm.sh/react` -> `/-/esm.sh/react`
  //  - `https://deno.land/std/path/mod.ts` -> `/-/deno.land/std/path/mod.ts`
  //  - `http://localhost:8080/mod.ts` -> `/-/http_localhost_8080/mod.ts`
  pub fn to_local_path(&self, url: &str) -> String {
    let url = Url::from_str(url).unwrap();
    let pathname = Path::new(url.path());
    let mut local_path = "/-/".to_owned();
    let scheme = url.scheme();
    if scheme == "http" {
      local_path.push_str("http_");
    }
    local_path.push_str(url.host_str().unwrap());
    if let Some(port) = url.port() {
      if scheme == "http" && port == 80 {
      } else if scheme == "https" && port == 443 {
      } else {
        local_path.push('_');
        local_path.push_str(port.to_string().as_str());
      }
    }
    local_path.push_str(&pathname.to_owned().to_slash().unwrap().to_string());
    if let Some(query) = url.query() {
      local_path.push('?');
      local_path.push_str(query);
    }
    local_path
  }

  /// Resolve import/export URLs.
  pub fn resolve(&mut self, url: &str, dynamic: bool, loc: Option<Span>) -> String {
    let referrer = if self.specifier_is_remote {
      Url::from_str(self.specifier.as_str()).unwrap()
    } else {
      Url::from_str(&("file://".to_owned() + self.specifier.trim_start_matches('.'))).unwrap()
    };
    let resolved_url = if let Ok(ret) = self.import_map.resolve(url, &referrer) {
      ret.to_string()
    } else {
      url.into()
    };
    let mut import_url = if resolved_url.starts_with("file://") {
      let path = resolved_url.strip_prefix("file://").unwrap();
      if !self.specifier_is_remote {
        let mut buf = PathBuf::from(self.specifier.trim_start_matches('.'));
        buf.pop();
        let mut path = diff_paths(&path, buf).unwrap().to_slash().unwrap().to_string();
        if !path.starts_with("./") && !path.starts_with("../") {
          path = "./".to_owned() + &path
        }
        path
      } else {
        ".".to_owned() + path
      }
    } else {
      resolved_url.clone()
    };
    let mut fixed_url: String = if resolved_url.starts_with("file://") {
      ".".to_owned() + resolved_url.strip_prefix("file://").unwrap()
    } else {
      resolved_url.into()
    };
    let is_remote = is_http_url(&fixed_url);

    if self.is_dev && is_esm_sh_url(&fixed_url) && !fixed_url.ends_with(".development.js") {
      if fixed_url.contains("?") {
        fixed_url = fixed_url + "&dev"
      } else {
        fixed_url = fixed_url + "?dev"
      }
      import_url = fixed_url.clone();
    }

    if is_css_url(&import_url) {
      if import_url.contains("?") {
        import_url = import_url + "&module"
      } else {
        import_url = import_url + "?module"
      }
    }

    if is_remote {
      // fix remote url to local path if allowed
      if self.resolve_remote_deps {
        import_url = self.to_local_path(&import_url);
      }
    } else {
      // apply graph version if exists
      let v = if self.graph_versions.contains_key(&fixed_url) {
        self.graph_versions.get(&fixed_url)
      } else {
        self.global_version.as_ref()
      };
      if let Some(version) = v {
        if import_url.contains("?") {
          import_url = format!("{}&v={}", import_url, version);
        } else {
          import_url = format!("{}?v={}", import_url, version);
        }
      }
    }

    // update dep graph
    self.deps.push(DependencyDescriptor {
      specifier: fixed_url.clone(),
      import_url: import_url.clone(),
      loc,
      dynamic,
    });

    import_url
  }
}

pub fn is_http_url(url: &str) -> bool {
  return url.starts_with("https://") || url.starts_with("http://");
}

pub fn is_esm_sh_url(url: &str) -> bool {
  return url.starts_with("https://esm.sh/") || url.starts_with("http://esm.sh/");
}

pub fn is_css_url(url: &str) -> bool {
  if is_esm_sh_url(url) {
    let url = Url::from_str(url).unwrap();
    for (key, _value) in url.query_pairs() {
      if key.eq("css") {
        return true;
      }
    }
  }
  return url.ends_with(".css") || url.contains(".css?");
}

fn is_false(value: &bool) -> bool {
  return !*value;
}
