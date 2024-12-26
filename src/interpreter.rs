use std::{
    collections::HashSet,
    fmt::{LowerHex, Pointer, UpperHex},
    sync::{Arc, LazyLock, Mutex},
};

use mlua::{FromLua, IntoLua};
use relative_path::RelativePathBuf;
use thiserror::Error;

use crate::parser::{Directive, ParseError};

// this is to allow adding resources from Lua code, i have no idea what other way to do this
pub(crate) static NEW_RESOURCE_PATHS: LazyLock<Mutex<Vec<Directive>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

#[derive(Error, Debug)]
enum ColorError {
    #[error("value `{0}` does not fit within {1} channels")]
    ValueOutOfBounds(u32, u8),
    #[error("invalid channel count `{0}`")]
    InvalidChannels(u8),
    #[error("cannot apply negative() to RGBA color")]
    NegativeRGBA,
    #[error("cannot perform arithmetic on two colors with different channels")]
    ArithmeticChannelsMismatch,
    #[error("color addition caused one of the channels to overflow past 255")]
    ArithmeticOverflow,
    #[error("color subtraction caused one of the channels to underflow below 0")]
    ArithmeticUnderflow,
}

#[derive(Debug, PartialEq, Eq, Clone, FromLua)]
pub struct RGB(u8, u8, u8);

impl RGB {
    fn from_value(value: u32) -> Self {
        Self(
            u8::try_from((value & 0xff0000) >> 16).unwrap(),
            u8::try_from((value & 0x00ff00) >> 8).unwrap(),
            u8::try_from(value & 0x0000ff).unwrap(),
        )
    }

    pub fn value(&self) -> u32 {
        ((self.0 as u32) << 16) + ((self.1 as u32) << 8) + (self.2 as u32)
    }

    pub fn value_rev(&self) -> u32 {
        ((self.2 as u32) << 16) + ((self.1 as u32) << 8) + (self.0 as u32)
    }

    fn arr(&self) -> String {
        format!("{} {} {}", self.0, self.1, self.2)
    }

    fn with_alpha(&self, alpha: u8) -> RGBA {
        RGBA(self.0, self.1, self.2, alpha)
    }

    fn add(&self, other: &Self) -> Result<Self, ColorError> {
        Ok(Self(
            self.0
                .checked_add(other.0)
                .ok_or(ColorError::ArithmeticOverflow)?,
            self.1
                .checked_add(other.1)
                .ok_or(ColorError::ArithmeticOverflow)?,
            self.2
                .checked_add(other.2)
                .ok_or(ColorError::ArithmeticOverflow)?,
        ))
    }

    fn sub(&self, other: &Self) -> Result<Self, ColorError> {
        Ok(Self(
            self.0
                .checked_sub(other.0)
                .ok_or(ColorError::ArithmeticUnderflow)?,
            self.1
                .checked_sub(other.1)
                .ok_or(ColorError::ArithmeticUnderflow)?,
            self.2
                .checked_sub(other.2)
                .ok_or(ColorError::ArithmeticUnderflow)?,
        ))
    }

    /// Subtract 0x1000000 from the reversed value. Used in *.ReaperTheme when a color has a togglable
    /// option, e.g. `col_main_bg` and `col_seltrack2`
    fn negative(&self) -> i64 {
        self.value_rev() as i64 - 0x1000000
    }
}

impl UpperHex for RGB {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02X}{:02X}{:02X}", self.2, self.1, self.0)
    }
}

impl LowerHex for RGB {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02x}{:02x}{:02x}", self.2, self.1, self.0)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, FromLua)]
pub struct RGBA(u8, u8, u8, u8);

impl RGBA {
    fn from_value(value: u32) -> Self {
        Self(
            u8::try_from((value & 0xff000000) >> 24).unwrap(),
            u8::try_from((value & 0x00ff0000) >> 16).unwrap(),
            u8::try_from((value & 0x0000ff00) >> 8).unwrap(),
            u8::try_from(value & 0x000000ff).unwrap(),
        )
    }

    pub fn value(&self) -> u32 {
        ((self.0 as u32) << 24) + ((self.1 as u32) << 16) + ((self.2 as u32) << 8) + (self.3 as u32)
    }

    pub fn value_rev(&self) -> u32 {
        ((self.3 as u32) << 24) + ((self.2 as u32) << 16) + ((self.1 as u32) << 8) + (self.0 as u32)
    }

    fn arr(&self) -> String {
        format!("{} {} {} {}", self.0, self.1, self.2, self.3)
    }

    fn with_alpha(&self, alpha: u8) -> RGBA {
        RGBA(self.0, self.1, self.2, alpha)
    }

    fn add(&self, other: &Self) -> Result<Self, ColorError> {
        Ok(Self(
            self.0
                .checked_add(other.0)
                .ok_or(ColorError::ArithmeticOverflow)?,
            self.1
                .checked_add(other.1)
                .ok_or(ColorError::ArithmeticOverflow)?,
            self.2
                .checked_add(other.2)
                .ok_or(ColorError::ArithmeticOverflow)?,
            self.3
                .checked_add(other.3)
                .ok_or(ColorError::ArithmeticOverflow)?,
        ))
    }

    fn sub(&self, other: &Self) -> Result<Self, ColorError> {
        Ok(Self(
            self.0
                .checked_sub(other.0)
                .ok_or(ColorError::ArithmeticUnderflow)?,
            self.1
                .checked_sub(other.1)
                .ok_or(ColorError::ArithmeticUnderflow)?,
            self.2
                .checked_sub(other.2)
                .ok_or(ColorError::ArithmeticUnderflow)?,
            self.3
                .checked_sub(other.3)
                .ok_or(ColorError::ArithmeticUnderflow)?,
        ))
    }

    fn to_rgb(&self) -> RGB {
        RGB(self.0, self.1, self.2)
    }
}

impl UpperHex for RGBA {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:02X}{:02X}{:02X}{:02X}",
            self.3, self.2, self.1, self.0
        )
    }
}

impl LowerHex for RGBA {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:02x}{:02x}{:02x}{:02x}",
            self.3, self.2, self.1, self.0
        )
    }
}

impl mlua::UserData for RGB {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        // methods
        methods.add_method("arr", |_, this, _value: ()| Ok(this.arr()));
        methods.add_method("with_alpha", |_, this, (alpha,): (u8,)| {
            Ok(this.with_alpha(alpha))
        });
        methods.add_method("negative", |_, this, _value: ()| Ok(this.negative()));
        methods.add_method("hex", |_, this, _value: ()| Ok(format!("{:X}", this)));

        // metamethods
        methods.add_meta_method(mlua::MetaMethod::Add, |_, this, other: RGB| {
            this.add(&other)
                .map_err(|err| mlua::Error::ExternalError(Arc::new(err)))
        });
        methods.add_meta_method(mlua::MetaMethod::Sub, |_, this, other: RGB| {
            this.sub(&other)
                .map_err(|err| mlua::Error::ExternalError(Arc::new(err)))
        });
    }
}

impl mlua::UserData for RGBA {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        // methods
        methods.add_method("arr", |_, this, _value: ()| Ok(this.arr()));
        methods.add_method("with_alpha", |_, this, (alpha,): (u8,)| {
            Ok(this.with_alpha(alpha))
        });
        methods.add_method("to_rgb", |_, this, _value: ()| Ok(this.to_rgb()));
        methods.add_method("hex", |_, this, _value: ()| Ok(format!("{:X}", this)));

        // metamethods
        methods.add_meta_method(mlua::MetaMethod::Add, |_, this, other: RGBA| {
            this.add(&other)
                .map_err(|err| mlua::Error::ExternalError(Arc::new(err)))
        });
        methods.add_meta_method(mlua::MetaMethod::Sub, |_, this, other: RGBA| {
            this.sub(&other)
                .map_err(|err| mlua::Error::ExternalError(Arc::new(err)))
        });
    }
}

enum Color {
    RGB(RGB),
    RGBA(RGBA),
}

impl Color {
    fn from_value(value: u32) -> Result<Self, ColorError> {
        if value <= 0xffffff {
            Self::new_with_channels(value, 3)
        } else {
            Self::new_with_channels(value, 4)
        }
    }

    fn new_with_channels(value: u32, channels: u8) -> Result<Self, ColorError> {
        match channels {
            3 => {
                if value <= 0xffffff {
                    Ok(Self::RGB(RGB(
                        u8::try_from((value & 0xff0000) >> 16).unwrap(),
                        u8::try_from((value & 0x00ff00) >> 8).unwrap(),
                        u8::try_from(value & 0x0000ff).unwrap(),
                    )))
                } else {
                    Err(ColorError::ValueOutOfBounds(value, 3))
                }
            }
            4 => Ok(Self::RGBA(RGBA(
                u8::try_from((value & 0xff000000) >> 24).unwrap(),
                u8::try_from((value & 0x00ff0000) >> 16).unwrap(),
                u8::try_from((value & 0x0000ff00) >> 8).unwrap(),
                u8::try_from(value & 0x000000ff).unwrap(),
            ))),
            x => Err(ColorError::InvalidChannels(x)),
        }
    }
}

impl IntoLua for Color {
    fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        let userdata = match self {
            Color::RGB(x) => lua.create_userdata(x),
            Color::RGBA(x) => lua.create_userdata(x),
        }?;
        Ok(mlua::Value::UserData(userdata))
    }
}

fn unset(table: &mlua::Table, key: &str) {
    table.set(key, None::<bool>).unwrap();
}

fn whitelist(table: &mlua::Table, keys: Vec<&str>) {
    let keys: HashSet<&str> = HashSet::from_iter(keys.into_iter());
    table
        .for_each(|k: String, _v: mlua::Value| {
            if !keys.contains(k.as_str()) {
                unset(&table, k.as_str());
            }
            Ok(())
        })
        .unwrap();
}

fn sandbox_lua(lua: &mlua::Lua) {
    let globals = lua.globals();

    // unset globals for sandboxing
    unset(&globals, "io");
    unset(&globals, "package");
    unset(&globals, "debug");
    unset(&globals, "dofile");
    unset(&globals, "loadfile");
    unset(&globals, "require");

    whitelist(
        &globals.get("os").unwrap(),
        vec!["clock", "date", "difftime", "time"],
    );
}

pub fn new() -> mlua::Lua {
    // sandbox lua following Roblox's guide:
    // https://luau-lang.org/sandbox

    let lua = mlua::Lua::new();

    sandbox_lua(&lua);

    {
        let globals = lua.globals();

        // additional functions for Reaper themes
        let func = lua
            .create_function(|_, (value, channels): (u32, Option<u8>)| {
                let result = if let Some(channels) = channels {
                    Color::new_with_channels(value, channels)
                } else {
                    Color::from_value(value)
                };
                let color = result.map_err(|err| mlua::Error::ExternalError(Arc::new(err)))?;
                Ok(color)
            })
            .unwrap();
        globals.set("color", func).unwrap();

        let func = lua
            .create_function(|_, (r, g, b): (u8, u8, u8)| Ok(RGB(r, g, b)))
            .unwrap();
        globals.set("rgb", func).unwrap();

        let func = lua
            .create_function(|_, (r, g, b, a): (u8, u8, u8, u8)| Ok(RGBA(r, g, b, a)))
            .unwrap();
        globals.set("rgba", func).unwrap();

        let func = lua
            .create_function(|_, (mode, frac): (String, f32)| {
                // the blend mode is a 18-bit value, split into multiple parts:
                //
                //     0b1 frac_____ mode____
                //     0b1 100000000 11111110

                // reaper's frac value is represented as a fraction: x / 256
                // we need to find the nearest x value
                if !(0f32 <= frac && frac <= 1f32) {
                    return Err(mlua::Error::RuntimeError(format!(
                        "frac `{}` must be a value between 0.0 and 1.0",
                        frac
                    )));
                }

                let frac: u32 = (frac * 256f32).round() as u32;

                // the mode value is just an enum
                let mode: u32 = match mode.as_str() {
                    "normal" => 0b00000000,
                    "add" => 0b00000001,
                    "overlay" => 0b00000100,
                    "multiply" => 0b00000011,
                    "dodge" => 0b00000010,
                    "hsv" => 0b11111110,
                    _ => {
                        return Err(mlua::Error::RuntimeError(format!(
                            // I'm formatting this string weirdly because cargo fmt refuses to
                            // format this file if i put the blend mode list in the base string
                            "mode `{}` must be one of: {}",
                            mode,
                            "\"normal\", \"add\", \"overlay\", \"multiply\", \"dodge\", \"hsv\""
                        )));
                    }
                };

                let result = 0b100000000000000000 + (frac << 8) + mode;

                Ok(result)
            })
            .unwrap();
        globals.set("blend", func).unwrap();

        // function to get an environment variable
        let func = lua
            .create_function(|_, (name,): (String,)| {
                std::env::var(&name).map_err(|err| mlua::Error::external(err))
            })
            .unwrap();
        globals.set("env", func).unwrap();

        // allow adding resouce in lua code
        let func = lua
            .create_function(|_, vals: mlua::Variadic<String>| -> mlua::Result<()> {
                if vals.len() == 1 {
                    let pattern = vals.get(0).unwrap();
                    let pattern = glob::Pattern::new(pattern).or(Err(mlua::Error::runtime(
                        format!("invalid glob pattern: {pattern}"),
                    )))?;

                    let dest = RelativePathBuf::from(".").normalize();

                    {
                        let mut paths = NEW_RESOURCE_PATHS.lock().unwrap();
                        paths.push(Directive::Resource { pattern, dest })
                    }

                    Ok(())
                } else if vals.len() == 2 {
                    let dest = vals.get(0).unwrap();
                    let dest = RelativePathBuf::from(dest).normalize();

                    let pattern = vals.get(1).unwrap();
                    let pattern = glob::Pattern::new(pattern).or(Err(mlua::Error::runtime(
                        format!("invalid glob pattern: {pattern}"),
                    )))?;

                    {
                        let mut paths = NEW_RESOURCE_PATHS.lock().unwrap();
                        paths.push(Directive::Resource { pattern, dest })
                    }

                    Ok(())
                } else {
                    Err(mlua::Error::runtime(
                        "resource(...) can only be called with 1 or 2 arguments",
                    ))
                }
            })
            .unwrap();
        globals.set("resource", func).unwrap();
    }

    lua
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb() {
        let lua = new();

        let result: RGB = lua.load("rgb(255, 255, 255)").eval().unwrap();
        let expected = RGB(255, 255, 255);
        assert_eq!(result, expected);

        let result: RGB = lua.load("rgb(1, 2, 3)").eval().unwrap();
        let expected = RGB(1, 2, 3);
        assert_eq!(result, expected);

        let result: RGBA = lua.load("rgba(255, 255, 255, 255)").eval().unwrap();
        let expected = RGBA(255, 255, 255, 255);
        assert_eq!(result, expected);

        let result: RGBA = lua.load("rgba(1, 2, 3, 4)").eval().unwrap();
        let expected = RGBA(1, 2, 3, 4);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_rgb_reuse() {
        let lua = new();

        lua.load("foo = rgb(11, 22, 33)").exec().unwrap();

        let _: RGB = lua.load("foo").eval().unwrap();

        // try to take the color again, this will fail if the user data destructed
        let _: RGB = lua.load("foo").eval().unwrap();

        // alternatively, assert that it fails:
        // let result: mlua::Result<Color> = lua.load("foo").eval();
        // assert!(matches!(
        //     result,
        //     mlua::Result::Err(mlua::Error::UserDataDestructed)
        // ))
    }

    #[test]
    fn test_arr() {
        let lua = new();

        let result: String = lua.load("rgb(1, 2, 3):arr()").eval().unwrap();
        let expected = "1 2 3";
        assert_eq!(result, expected);

        let result: String = lua.load("rgba(1, 2, 3, 4):arr()").eval().unwrap();
        let expected = "1 2 3 4";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_color() {
        let lua = new();

        let result: RGB = lua.load("color(0xffffff)").eval().unwrap();
        let expected = RGB(255, 255, 255);
        assert_eq!(result, expected);

        let result: RGBA = lua.load("color(0xffffff, 4)").eval().unwrap();
        let expected = RGBA(0, 255, 255, 255);
        assert_eq!(result, expected);

        let result: RGBA = lua.load("color(0x11223344)").eval().unwrap();
        let expected = RGBA(0x11, 0x22, 0x33, 0x44);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_blend() {
        let lua = new();

        let result: i32 = lua.load("blend('hsv', 0.12)").eval().unwrap();
        let expected = 0b100001111111111110;
        assert_eq!(result, expected);

        let result: i32 = lua.load("blend('normal', 0)").eval().unwrap();
        let expected = 0b100000000000000000;
        assert_eq!(result, expected);

        let result: i32 = lua.load("blend('normal', 1)").eval().unwrap();
        let expected = 0b110000000000000000;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_01() {
        // sandbox lua following Roblox's guide:
        // https://luau-lang.org/sandbox

        let lua = new();

        let map_table = lua.create_table().unwrap();
        map_table.set(1, "one").unwrap();
        map_table.set("two", 2).unwrap();

        lua.globals()
            .for_each(|k: mlua::Value, v: mlua::Value| {
                dbg!((k, v));
                Ok(())
            })
            .unwrap();

        lua.globals().set("map_table", map_table).unwrap();

        let result: Option<String> = lua
            .load(
                r"
                function add(a, b)
                    return a + b
                end

                return add(1, 3)
            ",
            )
            .eval()
            .unwrap();

        dbg!(result);

        let result: Option<String> = lua.load(r"add(3, 3)").eval().unwrap();

        dbg!(result);
    }
}
