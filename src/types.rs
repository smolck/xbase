#[cfg(feature = "lua")]
use mlua::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize, strum::Display)]
#[serde(untagged)]
pub enum XConfiguration {
    #[default]
    Debug,
    Release,
    Custom(String),
}

#[cfg(feature = "lua")]
impl<'a> FromLua<'a> for XConfiguration {
    fn from_lua(lua_value: LuaValue<'a>, _lua: &'a Lua) -> LuaResult<Self> {
        if let LuaValue::String(config) = lua_value {
            let value = config.to_str()?;
            Ok(match value {
                "debug" | "Debug" => Self::Debug,
                "release" | "Release" => Self::Release,
                _ => Self::Custom(value.to_string()),
            })
        } else if matches!(lua_value, LuaValue::Nil) {
            Ok(Self::Debug)
        } else {
            Err(LuaError::external("Expected a table got XConfiguration"))
        }
    }
}

/// Xcode Scheme
///
/// An Xcode scheme defines a collection of targets to build, a configuration to use when building,
/// and a collection of tests to execute.
pub type XScheme = String;

/// Xcode Target
///
/// A target specifies a product to build and contains the instructions for building the product
/// from a set of files in a project or workspace.
pub type XTarget = String;

/// Fields required to build a project
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct BuildConfiguration {
    /// TODO(nvim): make build config sysroot default to tmp in auto-build
    pub sysroot: Option<String>,
    /// Target to build
    pub target: Option<XTarget>,
    /// Configuration to build with, default Debug
    #[serde(default)]
    pub configuration: XConfiguration,
    /// Scheme to build with
    pub scheme: Option<XScheme>,
}
impl std::fmt::Display for BuildConfiguration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "xcodebuild")?;
        write!(f, " -configuration {}", self.configuration)?;

        if let Some(ref sysroot) = self.sysroot {
            write!(f, " -sysroot {sysroot}")?;
        }
        if let Some(ref scheme) = self.scheme {
            write!(f, " -scheme {scheme}")?;
        }
        if let Some(ref target) = self.target {
            write!(f, " -target {target}")?;
        }
        Ok(())
    }
}

#[cfg(feature = "lua")]
impl<'a> FromLua<'a> for BuildConfiguration {
    fn from_lua(lua_value: LuaValue<'a>, _lua: &'a Lua) -> LuaResult<Self> {
        if let LuaValue::Table(table) = lua_value {
            Ok(Self {
                sysroot: table.get("sysroot")?,
                target: table.get("target")?,
                configuration: table.get("configuration")?,
                scheme: table.get("scheme")?,
            })
        } else {
            Ok(BuildConfiguration::default())
        }
    }
}
