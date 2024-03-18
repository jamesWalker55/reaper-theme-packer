use std::{collections::HashSet, sync::Arc};

use mlua::FromLua;
use thiserror::Error;

#[derive(Debug, PartialEq, Eq, Clone, FromLua)]
pub enum Color {
    RGB(u8, u8, u8),
    RGBA(u8, u8, u8, u8),
}

#[derive(Error, Debug)]
enum ColorError {
    #[error("value `{0}` does not fit within {1} channels")]
    ValueOutOfBounds(u32, u8),
    #[error("invalid channel count `{0}`")]
    InvalidChannels(u8),
}

impl Color {
    fn new(value: u32) -> Result<Self, ColorError> {
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
                    Ok(Self::RGB(
                        u8::try_from((value & 0xff0000) >> 16).unwrap(),
                        u8::try_from((value & 0x00ff00) >> 8).unwrap(),
                        u8::try_from(value & 0x0000ff).unwrap(),
                    ))
                } else {
                    Err(ColorError::ValueOutOfBounds(value, 3))
                }
            }
            4 => Ok(Self::RGBA(
                u8::try_from((value & 0xff000000) >> 24).unwrap(),
                u8::try_from((value & 0x00ff0000) >> 16).unwrap(),
                u8::try_from((value & 0x0000ff00) >> 8).unwrap(),
                u8::try_from(value & 0x000000ff).unwrap(),
            )),
            x => Err(ColorError::InvalidChannels(x)),
        }
    }

    fn channels(&self) -> u8 {
        match self {
            Self::RGB(..) => 3,
            Self::RGBA(..) => 4,
        }
    }

    pub fn value(&self) -> i32 {
        match self {
            Self::RGB(r, g, b) => ((*r as i32) << 16) + ((*g as i32) << 8) + (*b as i32),
            Self::RGBA(r, g, b, a) => {
                ((*r as i32) << 24) + ((*g as i32) << 16) + ((*b as i32) << 8) + (*a as i32)
            }
        }
    }

    pub fn value_rev(&self) -> i32 {
        match self {
            Self::RGB(r, g, b) => ((*b as i32) << 16) + ((*g as i32) << 8) + (*r as i32),
            Self::RGBA(r, g, b, a) => {
                ((*a as i32) << 24) + ((*b as i32) << 16) + ((*g as i32) << 8) + (*r as i32)
            }
        }
    }

    fn arr(&self) -> String {
        match self {
            Self::RGB(r, g, b) => format!("{r} {g} {b}"),
            Self::RGBA(r, g, b, a) => format!("{r} {g} {b} {a}"),
        }
    }
}

impl mlua::UserData for Color {
    fn add_methods<'lua, M: mlua::UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("arr", |_, this, value: ()| Ok(this.arr()));
    }
}

fn unset(table: &mlua::Table, key: &str) {
    table.set(key, None::<bool>).unwrap();
}

fn whitelist(table: &mlua::Table, keys: Vec<&str>) {
    let keys: HashSet<&str> = HashSet::from_iter(keys.into_iter());
    table
        .for_each(|k: String, v: mlua::Value| {
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
                    Color::new(value)
                };
                let color = result.map_err(|err| mlua::Error::ExternalError(Arc::new(err)))?;
                Ok(color)
            })
            .unwrap();
        globals.set("color", func).unwrap();

        let func = lua
            .create_function(|_, (r, g, b): (u8, u8, u8)| Ok(Color::RGB(r, g, b)))
            .unwrap();
        globals.set("rgb", func).unwrap();

        let func = lua
            .create_function(|_, (r, g, b, a): (u8, u8, u8, u8)| Ok(Color::RGBA(r, g, b, a)))
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
    }

    lua
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb() {
        let lua = new();

        let result: Color = lua.load("rgb(255, 255, 255)").eval().unwrap();
        let expected = Color::RGB(255, 255, 255);
        assert_eq!(result, expected);

        let result: Color = lua.load("rgb(1, 2, 3)").eval().unwrap();
        let expected = Color::RGB(1, 2, 3);
        assert_eq!(result, expected);

        let result: Color = lua.load("rgba(255, 255, 255, 255)").eval().unwrap();
        let expected = Color::RGBA(255, 255, 255, 255);
        assert_eq!(result, expected);

        let result: Color = lua.load("rgba(1, 2, 3, 4)").eval().unwrap();
        let expected = Color::RGBA(1, 2, 3, 4);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_rgb_reuse() {
        let lua = new();

        lua.load("foo = rgb(11, 22, 33)").exec().unwrap();

        let _: Color = lua.load("foo").eval().unwrap();

        // try to take the color again, this will fail if the user data destructed
        let _: Color = lua.load("foo").eval().unwrap();

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

        let result: Color = lua.load("color(0xffffff)").eval().unwrap();
        let expected = Color::RGB(255, 255, 255);
        assert_eq!(result, expected);

        let result: Color = lua.load("color(0xffffff, 4)").eval().unwrap();
        let expected = Color::RGBA(0, 255, 255, 255);
        assert_eq!(result, expected);

        let result: Color = lua.load("color(0x11223344)").eval().unwrap();
        let expected = Color::RGBA(0x11, 0x22, 0x33, 0x44);
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
