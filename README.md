# Anyrun

A wayland native krunner-like runner, made with customizability in mind.


## Features

- Style customizability with GTK4 CSS
  - More info in [Styling](#Styling)
- Can do basically anything
  - As long as it can work with input and selection
  - Hence the name anyrun
- Easy to make plugins
  - You only need 4 functions!
  - See [Rink](plugins/rink) for a simple example. More info in the
    documentation of the [anyrun-plugin](anyrun-plugin) crate.
- Responsive
  - Asynchronous running of plugin functions
- Wayland native
  - GTK layer shell for overlaying the window
  - data-control for managing the clipboard

## Usage

### Dependencies

Anyrun mainly depends various GTK libraries, and rust of course for building the
project. Rust you can get with [rustup](https://rustup.rs). The rest are
statically linked in the binary. Here are the libraries you need to have to
build & run it:

- `gtk-layer-shell (libgtk-layer-shell)`
- `gtk3 (libgtk-3 libgdk-3)`
- `pango (libpango-1.0)`
- `cairo (libcairo libcairo-gobject)`
- `gdk-pixbuf2 (libgdk_pixbuf-2.0)`
- `glib2 (libgobject-2.0 libgio-2.0 libglib-2.0)`

## Installation

[![Packaging status](https://repology.org/badge/vertical-allrepos/anyrun.svg)](https://repology.org/project/anyrun/versions)

### Nix

An Anyrun package that contains all the official plugins is available in [nixpkgs](https://search.nixos.org/packages?channel=unstable&show=anyrun&from=0&size=50&sort=relevance&type=packages&query=anyrun).

#### Home-Manager module

The preferred way to use Home-Manager with Anyrun is by using the upstream module.

You may use it in your system like this:

```nix
{
  programs.anyrun = {
    enable = true;
    config = {
      x = { fraction = 0.5; };
      y = { fraction = 0.3; };
      width = { fraction = 0.3; };
      hideIcons = false;
      ignoreExclusiveZones = false;
      layer = "overlay";
      hidePluginInfo = false;
      closeOnClick = false;
      showResultsImmediately = false;
      maxEntries = null;

      plugins = [
        "${pkgs.anyrun}/lib/libapplications.so"
        "${pkgs.anyrun}/lib/libsymbols.so"
      ];
    };

    # Inline comments are supported for language injection into
    # multi-line strings with Treesitter! (Depends on your editor)
    extraCss = /*css */ ''
      .some_class {
        background: red;
      }
    '';

    extraConfigFiles."some-plugin.ron".text = ''
      Config(
        // for any other plugin
        // this file will be put in ~/.config/anyrun/some-plugin.ron
        // refer to docs of xdg.configFile for available options
      )
    '';
  };
}
```

Anyrun packages are built and cached automatically. To avoid unnecessary
recompilations, you may use the binary cache.

```nix
nix.settings = {
    builders-use-substitutes = true;
    extra-substituters = [
        "https://anyrun.cachix.org"
    ];

    extra-trusted-public-keys = [
        "anyrun.cachix.org-1:pqBobmOjI7nKlsUMV25u9QHa9btJK65/C8vnO3p346s="
    ];
};
```

> [!WARNING]
> While using the Anyrun flake, overriding the `nixpkgs` input for Anyrun will
> cause cache hits, i.e., you will have to build from source every time. To use
> the cache, do _not_ override the Nixpkgs input.

### Manual installation

Make sure all of the dependencies are installed, and then run the following
commands in order:

```bash
# Clone the repository and move to the cloned location
git clone https://github.com/anyrun-org/anyrun && cd anyrun

# Build all packages, and install the Anyrun binary
cargo build --release
cargo install --path anyrun/

# Create the config directory and the plugins subdirectory
mkdir -p ~/.config/anyrun/plugins

# Copy all of the built plugins to the correct directory
cp target/release/*.so ~/.config/anyrun/plugins

# Copy the default config file
cp examples/config.ron ~/.config/anyrun/config.ron
```

## Plugins

Anyrun requires plugins to function, as they provide the results for input. The
list of plugins in this repository is as follows:

- [Applications](plugins/applications/README.md)
  - Search and run system & user specific desktop entries.
- [Symbols](plugins/symbols/README.md)
  - Search unicode symbols.
- [Rink](plugins/rink/README.md)
  - Calculator & unit conversion.
- [Shell](plugins/shell/README.md)
  - Run shell commands.
- [Translate](plugins/translate/README.md)
  - Quickly translate text.
- [Kidex](plugins/kidex/README.md)
  - File search provided by [Kidex](https://github.com/Kirottu/kidex).
- [Randr](plugins/randr/README.md)
  - Rotate and resize; quickly change monitor configurations on the fly.
  - TODO: Only supports Hyprland, needs support for other compositors.
- [Stdin](plugins/stdin/README.md)
  - Turn Anyrun into a dmenu-like fuzzy selector.
  - Should generally be used exclusively with the `--plugins` argument.
- [Dictionary](plugins/dictionary/README.md)
  - Look up definitions for words
- [Websearch](plugins/websearch/README.md)
  - Search the web with configurable engines: Google, Ecosia, Bing, DuckDuckGo.
- [Nix-run](plugins/nix-run/README.md)
  - `nix run` graphical applications straight from Anyrun.
- [niri-focus](plugins/niri-focus/README.md)
  - Focus active windows with fuzzy search on niri.

## Configuration

The default configuration directory is `$HOME/.config/anyrun` the structure of
the config directory is as follows and should be respected by plugins:

```
- anyrun/
  - plugins/
    - <plugin dynamic libraries>
  - config.ron
  - style.css
  - <any plugin specific config files>
```

The [default config file](examples/config.ron) contains the default values, and
annotates all configuration options with comments on what they are and how to
use them.

## Styling

Anyrun supports [GTK4 CSS](https://docs.gtk.org/gtk4/css-properties.html) styling.
The style classes and widgets that use them are as follows:

- No class, unique widget:
  - `GtkText`: The main entry box
  - `GtkWindow`: The main window
- `.main`:
  - `GtkBox`: The box that contains everything else
- `.matches`:
  - `GtkBox`: The box that contains all the results & info boxes
- `.plugin`:
  - `GtkBox`: The main plugin box
  - `.info`:
    - `GtkBox`: Box containing the plugin info
    - `GtkImage`: Icon of the plugin
    - `GtkLabel`: Name of the plugin
- `.match`:
  - `GtkBox`: The box containing all contents of a match
  - `GtkImage`: The icon (if present)
  - `.title`:
    - `GtkLabel`: The title
  - `.description`
    - `GtkLabel`: The description (if present)

Refer to the [default style](anyrun/res/style.css) for an example, and use `GTK_DEBUG=interactive anyrun`
to edit styles live.

## Arguments

The custom arguments for anyrun are as follows:

- `--config-dir`, `-c`: Override the configuration directory

The rest of the arguments are automatically generated based on the config, and
can be used to override configuration parameters. For example if you want to
temporarily only run the Applications and Symbols plugins on the top side of the
screen, you would run
`anyrun --plugins libapplications.so --plugins libsymbols.so --position top`.

# Plugin development

The plugin API is intentionally very simple to use. This is all you need for a
plugin:

`Cargo.toml`:

```toml
#[package] omitted
[lib]
crate-type = ["cdylib"] # Required to build a dynamic library that can be loaded by anyrun

[dependencies]
anyrun-plugin = { git = "https://github.com/anyrun-org/anyrun" }
abi_stable = "0.11.1"
# Any other dependencies you may have
```

`lib.rs`:

```rs
use abi_stable::std_types::{RString, RVec, ROption};
use anyrun_plugin::*;

#[init]
fn init(config_dir: RString) {
  // Your initialization code. This is run in another thread.
  // The return type is the data you want to share between functions
}

#[info]
fn info() -> PluginInfo {
  PluginInfo {
    name: "Demo".into(),
    icon: "help-about".into(), // Icon from the icon theme
  }
}

#[get_matches]
fn get_matches(input: RString) -> RVec<Match> {
  // The logic to get matches from the input text in the `input` argument.
  // The `data` is a mutable reference to the shared data type later specified.
  vec![Match {
    title: "Test match".into(),
    icon: ROption::RSome("help-about".into()),
    use_pango: false,
    description: ROption::RSome("Test match for the plugin API demo".into()),
    id: ROption::RNone, // The ID can be used for identifying the match later, is not required
  }].into()
}

#[handler]
fn handler(selection: Match) -> HandleResult {
  // Handle the selected match and return how anyrun should proceed
  HandleResult::Close
}
```

And that's it! That's all of the API needed to make runners. Refer to the
plugins in the [plugins](plugins) folder for more examples.
