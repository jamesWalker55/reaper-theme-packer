use std::{collections::HashSet, sync::Arc};

use mlua::FromLua;
use terrors::OneOf;
use thiserror::Error;

#[derive(Debug, PartialEq, Eq, Clone, FromLua)]
pub enum Color {
    RGB(u8, u8, u8),
    RGBA(u8, u8, u8, u8),
}

// #[derive(Error, Debug)]
// enum ColorError {
//     #[error("invalid channel count `{0}`")]
//     InvalidChannels(u8),
//     #[error("cannot apply negative() to RGBA color")]
//     NegativeRGBA,
//     #[error("cannot perform arithmetic on two colors with different channels")]
//     ArithmeticChannelsMismatch,
//     #[error("color addition caused one of the channels to overflow past 255")]
//     ArithmeticOverflow,
//     #[error("color subtraction caused one of the channels to underflow below 0")]
//     ArithmeticUnderflow,
// }

pub(crate) mod errors {
    use thiserror::Error;

    #[derive(Error, Debug)]
    #[error("value `{value}` does not fit within {channels} channels")]
    pub(crate) struct ValueOutOfBounds {
        pub(crate) value: u32,
        pub(crate) channels: u8,
    }

    #[derive(Error, Debug)]
    #[error("invalid channel count `{0}`")]
    pub(crate) struct InvalidChannels(pub(crate) u8);

    #[derive(Error, Debug)]
    #[error("cannot apply negative() to RGBA color")]
    pub(crate) struct NegativeRGBA;

    #[derive(Error, Debug)]
    #[error("cannot perform arithmetic on two colors with different channels")]
    pub(crate) struct ArithmeticChannelsMismatch;

    #[derive(Error, Debug)]
    #[error("color addition caused one of the channels to overflow past 255")]
    pub(crate) struct ArithmeticOverflow;

    #[derive(Error, Debug)]
    #[error("color subtraction caused one of the channels to underflow below 0")]
    pub(crate) struct ArithmeticUnderflow;
}

impl Color {
    fn new(value: u32) -> Result<Self, errors::ValueOutOfBounds> {
        let result = if value <= 0xffffff {
            Self::new_with_channels(value, 3)
        } else {
            Self::new_with_channels(value, 4)
        };
        result.map_err(|err| {
            let Ok(err) = err.narrow::<errors::ValueOutOfBounds, _>() else {
                panic!("channel configuration must be correct here");
            };
            err
        })
    }

    fn new_with_channels(
        value: u32,
        channels: u8,
    ) -> Result<Self, OneOf<(errors::ValueOutOfBounds, errors::InvalidChannels)>> {
        match channels {
            3 => {
                if value <= 0xffffff {
                    Ok(Self::RGB(
                        u8::try_from((value & 0xff0000) >> 16).unwrap(),
                        u8::try_from((value & 0x00ff00) >> 8).unwrap(),
                        u8::try_from(value & 0x0000ff).unwrap(),
                    ))
                } else {
                    Err(OneOf::new(errors::ValueOutOfBounds { value, channels: 3 }))
                }
            }
            4 => Ok(Self::RGBA(
                u8::try_from((value & 0xff000000) >> 24).unwrap(),
                u8::try_from((value & 0x00ff0000) >> 16).unwrap(),
                u8::try_from((value & 0x0000ff00) >> 8).unwrap(),
                u8::try_from(value & 0x000000ff).unwrap(),
            )),
            x => Err(OneOf::new(errors::InvalidChannels(x))),
        }
    }

    fn channels(&self) -> u8 {
        match self {
            Self::RGB(..) => 3,
            Self::RGBA(..) => 4,
        }
    }

    pub fn value(&self) -> u32 {
        match self {
            Self::RGB(r, g, b) => ((*r as u32) << 16) + ((*g as u32) << 8) + (*b as u32),
            Self::RGBA(r, g, b, a) => {
                ((*r as u32) << 24) + ((*g as u32) << 16) + ((*b as u32) << 8) + (*a as u32)
            }
        }
    }

    pub fn value_rev(&self) -> u32 {
        match self {
            Self::RGB(r, g, b) => ((*b as u32) << 16) + ((*g as u32) << 8) + (*r as u32),
            Self::RGBA(r, g, b, a) => {
                ((*a as u32) << 24) + ((*b as u32) << 16) + ((*g as u32) << 8) + (*r as u32)
            }
        }
    }

    /// Subtract 0x1000000 from the reversed value. Used in *.ReaperTheme when a color has a togglable
    /// option, e.g. `col_main_bg` and `col_seltrack2`
    fn negative(&self) -> Result<i64, errors::NegativeRGBA> {
        match self {
            Self::RGB(..) => Ok(self.value_rev() as i64 - 0x1000000),
            Self::RGBA(..) => Err(errors::NegativeRGBA),
        }
    }

    fn arr(&self) -> String {
        match self {
            Self::RGB(r, g, b) => format!("{r} {g} {b}"),
            Self::RGBA(r, g, b, a) => format!("{r} {g} {b} {a}"),
        }
    }

    fn with_alpha(&self, alpha: u8) -> Self {
        match self {
            Self::RGB(r, g, b) => Self::RGBA(*r, *g, *b, alpha),
            Self::RGBA(r, g, b, _a) => Self::RGBA(*r, *g, *b, alpha),
        }
    }

    fn is_rgb(&self) -> bool {
        match self {
            Color::RGB(..) => true,
            Color::RGBA(..) => false,
        }
    }

    fn is_rgba(&self) -> bool {
        match self {
            Color::RGB(..) => false,
            Color::RGBA(..) => true,
        }
    }

    fn add(
        &self,
        other: &Color,
    ) -> Result<
        Self,
        OneOf<(
            errors::ArithmeticOverflow,
            errors::ArithmeticChannelsMismatch,
        )>,
    > {
        match self {
            Color::RGB(r, g, b) => match other {
                Color::RGB(r2, g2, b2) => Ok(Color::RGB(
                    r.checked_add(*r2)
                        .ok_or(OneOf::new(errors::ArithmeticOverflow))?,
                    g.checked_add(*g2)
                        .ok_or(OneOf::new(errors::ArithmeticOverflow))?,
                    b.checked_add(*b2)
                        .ok_or(OneOf::new(errors::ArithmeticOverflow))?,
                )),
                Color::RGBA(..) => Err(OneOf::new(errors::ArithmeticChannelsMismatch)),
            },
            Color::RGBA(r, g, b, a) => match other {
                Color::RGB(..) => Err(OneOf::new(errors::ArithmeticChannelsMismatch)),
                Color::RGBA(r2, g2, b2, a2) => Ok(Color::RGBA(
                    r.checked_add(*r2)
                        .ok_or(OneOf::new(errors::ArithmeticOverflow))?,
                    g.checked_add(*g2)
                        .ok_or(OneOf::new(errors::ArithmeticOverflow))?,
                    b.checked_add(*b2)
                        .ok_or(OneOf::new(errors::ArithmeticOverflow))?,
                    a.checked_add(*a2)
                        .ok_or(OneOf::new(errors::ArithmeticOverflow))?,
                )),
            },
        }
    }

    fn sub(
        &self,
        other: &Color,
    ) -> Result<
        Self,
        OneOf<(
            errors::ArithmeticUnderflow,
            errors::ArithmeticChannelsMismatch,
        )>,
    > {
        match self {
            Color::RGB(r, g, b) => match other {
                Color::RGB(r2, g2, b2) => Ok(Color::RGB(
                    r.checked_sub(*r2)
                        .ok_or(OneOf::new(errors::ArithmeticUnderflow))?,
                    g.checked_sub(*g2)
                        .ok_or(OneOf::new(errors::ArithmeticUnderflow))?,
                    b.checked_sub(*b2)
                        .ok_or(OneOf::new(errors::ArithmeticUnderflow))?,
                )),
                Color::RGBA(..) => Err(OneOf::new(errors::ArithmeticChannelsMismatch)),
            },
            Color::RGBA(r, g, b, a) => match other {
                Color::RGB(..) => Err(OneOf::new(errors::ArithmeticChannelsMismatch)),
                Color::RGBA(r2, g2, b2, a2) => Ok(Color::RGBA(
                    r.checked_sub(*r2)
                        .ok_or(OneOf::new(errors::ArithmeticUnderflow))?,
                    g.checked_sub(*g2)
                        .ok_or(OneOf::new(errors::ArithmeticUnderflow))?,
                    b.checked_sub(*b2)
                        .ok_or(OneOf::new(errors::ArithmeticUnderflow))?,
                    a.checked_sub(*a2)
                        .ok_or(OneOf::new(errors::ArithmeticUnderflow))?,
                )),
            },
        }
    }
}

impl mlua::UserData for Color {
    fn add_methods<'lua, M: mlua::UserDataMethods<'lua, Self>>(methods: &mut M) {
        // methods
        methods.add_method("arr", |_, this, _value: ()| Ok(this.arr()));
        methods.add_method("negative", |_, this, _value: ()| {
            this.negative()
                .map_err(|err| mlua::Error::ExternalError(Arc::new(err)))
        });
        methods.add_method("with_alpha", |_, this, (alpha,): (u8,)| {
            Ok(this.with_alpha(alpha))
        });

        // metamethods
        methods.add_meta_method(mlua::MetaMethod::Add, |_, this, other: Color| {
            this.add(&other)
                .map_err(|err| mlua::Error::ExternalError(Arc::new(err)))
        });
        methods.add_meta_method(mlua::MetaMethod::Sub, |_, this, other: Color| {
            this.sub(&other)
                .map_err(|err| mlua::Error::ExternalError(Arc::new(err)))
        });
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
