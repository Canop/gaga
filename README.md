# gaga

**This program is written by an LLM** (Claude), directed and reviewed by a human.

gaga brings git shortcuts to the shell, in the spirit of scm-breeze: the status is printed with numbered files when you enter a repository, and those numbers can then be used in place of the paths.

    ~/dev> cd gaga
    main ↑1
    1  M src/cli.rs
    2 ?? notes.md
    ~/dev/gaga> ga 1
    main ↑1
    1 M  src/cli.rs
    2 ?? notes.md
    ~/dev/gaga> ge 2

It's a single binary, for zsh and bash. Its shell part is a function, a hook and a few aliases, which it generates itself.

## install

    cargo install --path .

Then run

    gaga install

It writes the configuration file, and adds the line gaga needs at the end of your `.zshrc`, or of your `.bashrc` and `.bash_profile`:

    eval "$(gaga init zsh)"

It only adds what's missing, and tells what it does. Run it again after an update of gaga to get the new aliases and commands: the entries you changed are kept.

## usage

`gg <command> <args>` runs a command. Among the arguments, a number like `3` or a range like `3-5` is replaced with the files of the last status printed in this shell. Write `./3` for a file named `3`.

| alias | command | effect |
|-|-|-|
| `gs` | `gg status` | print the status and number the files |
| `ga` | `gg add` | `git add`, then status |
| `gu` | `gg unstage` | remove from the index, then status |
| `gd` | `gg diff` | `git diff` |
| `gdc` | `gg diff --cached` | `git diff --cached` |
| `ge` | `gg e` | open in `$EDITOR` |
| `grm` | `gg rm` | delete, then status |

Arguments which aren't numbers are given unchanged to the command: `ga -p 2`, `gd --stat`.

The status is also printed on entering a repository, not when moving inside it.

Repositories are remembered when you enter them. `c <name>` jumps to the last visited one having this name, or whose name starts with it, or contains it. Names are completed with the tab key, and `c` alone lists them.

## configuration

The configuration is in `~/.config/gaga/config.toml`, where `gaga install` writes [the default one](defaults/default-config.toml).

This file defines all the aliases, the commands they call, and the name of the `c` function: change or remove what you want. `gg` without argument runs the command named `bare`, and `gg conf` opens the file in `$EDITOR`.

To add a `gg blame` command with a `gbl` alias, add `gbl = "blame"` in the `[aliases]` section, and this command:

    [commands.blame]
    command = ["git", "blame"]

The aliases and the `c` function are set by `gaga init`: after having changed them, open a new shell or source its rc file.

## state

The numbered files of each shell and the known repositories are kept in a SQLite database at `~/.local/state/gaga/gaga.db3`.

## licence

[0BSD](LICENSE)
