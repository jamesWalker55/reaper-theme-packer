use std::collections::HashSet;

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

    lua
}

#[cfg(test)]
mod tests {
    use super::*;

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
