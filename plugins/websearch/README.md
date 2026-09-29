# Websearch

Search with your preferred search-engine. You can configure multiple engines.

## Usage

Enter your search-term and select the resulting search action you want to perform.

> [!NOTE]
> 
> This plugin depends on `xdg-open`. Make sure it is installed and in `$PATH`.

## Configuration

Default config

```ron
Config(
  prefix: "?",
  // Options: Google, Ecosia, Bing, DuckDuckGo, Custom
  //
  // Custom engines can be defined as such:
  // Custom(
  //   name: "Searx",
  //   url: "searx.be/?q={}",
  //   icon: Some("/path/to/icon.png"),  //can be left out entirely
  // )
  //
  // NOTE: `{}` is replaced by the search query and `https://` is automatically added in front.
  engines: [Google] 
)
```

### Icons

The Custom engine icon can be one of:

- an icon name supported by your theme:
  ```ron
  Some("view-more-symbolic")
  ```
- an icon name from the [freedesktop.org icon naming spec](https://specifications.freedesktop.org/icon-naming/latest/):
  ```ron
  Some("help-about")
  ```
- an absolute path to an icon, e.g.
  ```ron
  Some("/absolute/path/to/icon.png")
  ```
