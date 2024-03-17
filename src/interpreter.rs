use std::{collections::HashSet, sync::Arc};

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

pub fn new() -> mlua::Lua {
    // sandbox lua following Roblox's guide:
    // https://luau-lang.org/sandbox

    let lua = mlua::Lua::new();

    {
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

        // additional functions for Reaper themes
        let func = lua
            .create_function(|_, (r, g, b): (u8, u8, u8)| {
                let r = i32::from(r);
                let g = i32::from(g);
                let b = i32::from(b);

                Ok((b << 16) + (g << 8) + r)
            })
            .unwrap();
        globals.set("rgb", func).unwrap();

        let func = lua
            .create_function(|_, (r, g, b, a): (u8, u8, u8, u8)| {
                let r = i32::from(r);
                let g = i32::from(g);
                let b = i32::from(b);
                let a = i32::from(a);

                Ok((a << 24) + (b << 16) + (g << 8) + r)
            })
            .unwrap();
        globals.set("rgba", func).unwrap();

        let func = lua
            .create_function(|_, (value, channels): (i64, Option<u8>)| {
                let channels = if let Some(channels) = channels {
                    channels
                } else {
                    let mut channels: u8 = 3;
                    while value >= (2i64).pow(u32::from(channels) * 8) {
                        channels += 1;
                    }
                    channels
                };

                if value >= (2i64).pow(u32::from(channels) * 8) {
                    return Err(mlua::Error::RuntimeError(format!(
                        "value {} does not fit within {} channels ({} bits)",
                        value,
                        channels,
                        channels * 8
                    )));
                }
                dbg!((&value, &channels));

                todo!()
            })
            .unwrap();
        globals.set("arr", func).unwrap();
    }

    lua
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb() {
        let lua = new();

        let result: i32 = lua.load("rgb(255, 255, 255)").eval().unwrap();
        let expected = 0xffffff;
        assert_eq!(result, expected);

        let result: i32 = lua.load("rgb(0, 255, 255)").eval().unwrap();
        let expected = 0xffff00;
        assert_eq!(result, expected);

        let result: i32 = lua.load("rgb(0, 0, 255)").eval().unwrap();
        let expected = 0xff0000;
        assert_eq!(result, expected);

        let result: Result<i32, _> = lua.load("rgb(256, 0, 255)").eval();
        assert!(result.is_err());
    }

    #[test]
    fn test_arr() {
        let lua = new();

        lua.load("arr(255)").exec().unwrap();
        lua.load("arr(0xffffff)").exec().unwrap();
        lua.load("arr(0xffffffff)").exec().unwrap();
        lua.load("arr(0xffffffff, 3)").exec().unwrap();
        // let expected = 0xffffff;
        // assert_eq!(result, expected);
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
