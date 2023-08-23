# Anyrun

A wayland native krunner-like runner, made with customizability in mind.

# Features

- Style customizability with GTK+ CSS
  - More info in [Styling](#Styling)
- Can do basically anything
  - As long as it can work with input and selection
  - Hence the name anyrun
- Responsive
  - Asynchronous running of plugin functions
- Wayland native
  - GTK layer shell for overlaying the window
  - data-control for managing the clipboard

# Usage

## Dependencies

Anyrun mainly depends various GTK libraries, and rust of course for building the project. Rust you can get with [rustup](https://rustup.rs). The rest are statically linked in the binary.
Here are the libraries you need to have to build & run it:

- `gtk-layer-shell (libgtk-layer-shell)`
- `gtk3 (libgtk-3 libgdk-3)`
- `pango (libpango-1.0)`
- `cairo (libcairo libcairo-gobject)`
- `gdk-pixbuf2 (libgdk_pixbuf-2.0)`
- `glib2 (libgobject-2.0 libgio-2.0 libglib-2.0)`

## Installation

[![Packaging status](https://repology.org/badge/vertical-allrepos/anyrun.svg)](https://repology.org/project/anyrun/versions)

### Nix

You can use the flake:

```nix
# flake.nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    anyrun.url = "github:Kirottu/anyrun";
    anyrun.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, anyrun }: let
  in {
    nixosConfigurations.HOSTNAME = nixpkgs.lib.nixosSystem {
      # ...

      system.packages = [ anyrun.packages.${system}.anyrun ];

      # ...
    };
  };
}
```

The flake provides multiple packages:

- anyrun (default) - just the anyrun binary
- anyrun-with-all-plugins - anyrun and all builtin plugins
- applications - the applications plugin
- dictionary - the dictionary plugin
- kidex - the kidex plugin
- randr - the randr plugin
- rink - the rink plugin
- shell - the shell plugin
- stdin - the stdin plugin
- symbols - the symbols plugin
- translate - the translate plugin
- websearch - the websearch plugin

#### home-manager module

We have a home-manager module available at `homeManagerModules.default`. You use it like this:

```nix
{
  programs.anyrun = {
    enable = true;
    config = {
      plugins = [
        # An array of all the plugins you want, which either can be paths to the .so files, or their packages
        inputs.anyrun.packages.${pkgs.system}.applications
        ./some_plugin.so
        "${inputs.anyrun.packages.${pkgs.system}.anyrun-with-all-plugins}/lib/kidex"
      ];
      width = { fraction = 0.3; };
      position = "top";
      verticalOffset = { absolute = 0; };
      hideIcons = false;
      ignoreExclusiveZones = false;
      layer = "overlay";
      hidePluginInfo = false;
      closeOnClick = false;
      showResultsImmediately = false;
      maxEntries = null;
    };
    extraCss = ''
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

You might also want to use the binary cache to avoid building locally.

```nix
nix.settings = {
    builders-use-substitutes = true;
    # substituters to use
    substituters = [
        "https://anyrun.cachix.org"
    ];

    trusted-public-keys = [
        "anyrun.cachix.org-1:pqBobmOjI7nKlsUMV25u9QHa9btJK65/C8vnO3p346s="
    ];
};
```

### Manual installation

Make sure all of the dependencies are installed, and then run the following commands in order:

```sh
git clone https://github.com/Kirottu/anyrun.git # Clone the repository
cd anyrun # Change the active directory to it
cargo build --release # Build all the packages
cargo install --path anyrun/ # Install the anyrun binary
mkdir -p ~/.config/anyrun/plugins # Create the config directory and the plugins subdirectory
cp target/release/*.so ~/.config/anyrun/plugins # Copy all of the built plugins to the correct directory
cp examples/config.ron ~/.config/anyrun/config.ron # Copy the default config file
```

## Plugins

Anyrun requires plugins to function, as they provide the results for input. The list of plugins in this repository is as follows:

### Official plugins

Plugins that are a part of the project, and actively maintained alongside the project.

- [Applications](https://github.com/anyrun-org/plugin-applications)
  - Search and run system & user specific desktop entries.
- [Symbols](https://github.com/anyrun-org/plugin-symbols)
  - Search unicode symbols.
- [Rink](https://github.com/anyrun-org/plugin-rink)
  - Calculator & unit conversion.
- [Shell](https://github.com/anyrun-org/plugin-shell)
  - Run shell commands.
- [Translate](https://github.com/anyrun-org/plugin-translate)
  - Quickly translate text.
- [Kidex](https://github.com/anyrun-org/plugin-kidex)
  - File search provided by [Kidex](https://github.com/Kirottu/kidex).
- [Randr](https://github.com/anyrun-org/plugin-randr)
  - Rotate and resize; quickly change monitor configurations on the fly.
  - TODO: Only supports Hyprland, needs support for other compositors.
- [Stdin](https://github.com/anyrun-org/plugin-stdin)
  - Turn Anyrun into a dmenu like fuzzy selector.
  - Should generally be used exclusively with the `-o` argument.
- [Dictionary](https://github.com/anyrun-org/plugin-dictionary)
  - Look up definitions for words
- [Websearch](https://github.com/anyrun-org/plugin-websearch)
  - Search the web with configurable engines: Google, Ecosia, Bing, DuckDuckGo.

### Community plugins

Plugins that are community contributed and/or more niche that are not officially a part of this project.

- [Randr](https://github.com/Kirottu/anyrun-plugin-randr)
  - Rotate and resize; quickly change monitor configurations on the fly.
  - TODO: Only supports Hyprland, needs support for other compositors.

## Configuration

The default configuration directory is `$HOME/.config/anyrun` the structure of the config directory is as follows and should be respected by plugins:

```
- anyrun
  - plugins
    <plugin dynamic libraries>
  config.ron
  style.css
  <any plugin specific config files>

```

The [default config file](examples/config.ron) contains the default values, and annotates all configuration options with comments on what they are and how to use them.

## Styling

Anyrun supports [GTK+ CSS](https://docs.gtk.org/gtk3/css-overview.html) styling. The names for the different widgets and widgets associated with them are as follows:

- `entry`: The entry box
  - `GtkEntry`
- `window`: The window
  - `GtkWindow`
- `main`: "Main" parts of the layout
  - `GtkListBox`: The main list containing the plugins
  - `GtkBox`: The box combining the main list and the entry box
- `plugin`: Anything for the entire plugin
  - `GtkLabel`: The name of the plugin
  - `GtkBox`: The different boxes in the plugin view
  - `GtkImage`: The icon of the plugin
- `match`: Widgets of a specific match
  - `GtkBox`: The main box of the match and the box containing the title and the description if present
  - `GtkImage`: The icon of the match (if present)
- `match-title`: Specific for the title of the match
  - `GtkLabel`
- `match-desc`: Specific for the description of the match
  - `GtkLabel`

## Arguments

The custom arguments for anyrun are as follows:

- `--config-dir`, `-c`: Override the configuration directory

The rest of the arguments are automatically generated based on the config, and can be used to override
configuration parameters. For example if you want to temporarily only run the Applications and Symbols plugins on
the top side of the screen, you would run `anyrun --plugins libapplications.so --plugins libsymbols.so --position top`.

# Plugin development

To develop plugins, refer to the [plugin crate](https://github.com/anyrun-org/plugin) for instructions on how to do that.
