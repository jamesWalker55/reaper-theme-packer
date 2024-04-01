# Reaper Theme Packer

A command-line program to "compile" multiple `*.rtconfig.txt`, `*.ini` and `*.lua` files into a single `*.ReaperThemeZip`.

**WARNING: Do not use this tool on untrusted theme sources!** This tool includes a Lua interpreter for simple scripting and expression evaluation. When compiling a theme, it evaluates and runs Lua code using the Lua interpreter. I removed most Lua functions that interact with the OS for safety, but I don't guarantee that this Lua interpreter is 100% safe from malicious code hidden in theme source files.

Example usage:

```sh
reaper-theme-packer ./example/index.rtconfig.txt ./example.ReaperThemeZip
```

## Introduction

This tool runs a preprocessor on a given `*.rtconfig.txt` file. The file may contain directives like `#include "..."` to add `rtconfig.txt` and `ini` (`ReaperTheme`) files to the theme. It can also `#include` Lua files to execute code to, for example, define helper functions or constants that can be used anywhere.

The [example](./example) folder contains a basic example theme.

## Example

**index.rtconfig.txt**

```plain
#include "constants.lua"
#include "stuff.ini"

#include "stuff.rtconfig.txt"

set tcp.volume.label.color [#{my_colors.blue:arr()}]
```

**constants.lua**

```lua
my_colors = {
    red = rgb(255, 0, 0),
    blue = color(0x0000ff),
}
```

**stuff.ini**

```plain
[color theme]
col_tr1_bg=#{rgb(11, 22, 33)}
col_tr2_bg=#{my_colors.red}
```

**stuff.rtconfig.txt**

```plain
front tcp.volume

set tcp.volume [1 2 3 4]
```

The above files will be compiled down into a `rtconfig.txt` and a `<your theme name>.ReaperTheme` file within the output zip:

**rtconfig.txt**

```plain

front tcp.volume

set tcp.volume [1 2 3 4]


set tcp.volume.label.color [0 0 255]
```

**\<your theme name\>.ReaperTheme**

```ini
[color theme]
col_tr1_bg=2168331
col_tr2_bg=255
```

## Lua Evaluation

Input:

```plain
# rtconfig
set foo [#{my_color:arr()}]

# ini
col_tr2_bg=#{my_other_color}
```

Output:

```plain
# rtconfig
set foo [11 22 33]

# ini
col_tr2_bg=2168331
```

In `rtconfig.txt` and `ini` files, text between `#{...}` are treated as Lua expressions and evaluated.

There are several built-in functions in the Lua interpreter:

### Built-in Functions

```ini
# ReaperTheme file
midi_itemctl_mode=#{blend("normal", 0.598)}
# => midi_itemctl_mode=170240
```

Generate a blending value used in ReaperTheme definitions.

- Available blending modes: `"normal", "add", "overlay", "multiply", "dodge", "hsv"`
- Fraction: Must be between 0.0 and 1.0

### Colors

I have added "color objects" to this program for easier manipulation and sharing of colors between rtconfig.txt and ReaperTheme files.

Colors may be either RGB or RGBA. They may be created using the `color`, `rgb`, `rgba` functions.

When evaluated, colors will be converted into a single number in `0xBBGGRR` format (or `0xAABBGGRR` for RGBA colors) since this format is what ReaperTheme uses.

**Methods:**

```lua
-- detect channels automatically
foo = color(0x112233)

-- create RGB color (3 channels)
foo = color(0x112233, 3)
-- create RGBA color (4 channels)
foo = color(0x11223344, 4)
```

Create a color using a number. You may manually specify whether to create a RGB or RGBA color by adding a second argument with either `3` or `4`.

```lua
foo = rgb(11, 22, 33)
foo = rgba(11, 22, 33, 44)
```

Create RGB and RGBA colors using individual values for each channel.

```lua
foo:arr()
```

Return a space-separated string containing each of the color channels. E.g. '11 22 33'

```lua
foo:with_alpha(alpha)
```

Set the alpha channel of the given color. For RGB colors, this converts the RGB color to an RGBA color.

```lua
foo:negative()
```

(For RGB colors only) Subtract 0x1000000 from the reversed value. Used in *.ReaperTheme when a color has a togglable option, e.g. `col_main_bg` and `col_seltrack2`

```lua
foo:to_rgb()
```

(For RGBA colors only) Convert an RGBA color to an RGB color by discarding its alpha channel.

## Directives

### include

```plain
#include "constants.lua"
#include "stuff.ini"
#include "stuff.rtconfig.txt"
```

Include another file relative to the current file. The behaviour depends on the extension of the imported file:

- `.rtconfig.txt`: Append it to the current rtconfig file
- `.ini`: Add its entries to the ReaperTheme output file
- `.lua`: Just evaluate the file using the Lua interpreter, global variables / functions can be used in other files

### resource

```plain
# Add all png images in ./tcp to the zip
#resource "tcp/*.png"

# Add all png images in ./tcp_2x to a subfolder './200' in the zip
#resource "200": "tcp_2x/*.png"
```

Add resources to the output ReaperThemeZip. The resources are specified with a glob pattern.
