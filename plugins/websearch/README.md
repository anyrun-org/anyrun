# Websearch

Search with your preferred search-engine. You can configure multiple engines.

## Usage

Enter your search-term and select the resulting search action you want to perform.

## Configuration

Default config

```ron
Config(
  prefix: "?",
  // Options: Google, Ecosia, Bing, DuckDuckGo, Custom
  //
  // Custom engines can be defined as such:
  // Custom {
  //   name: "Searx"
  //   url: "searx.be/?q="
  // }
  //
  // NOTE: The search query is appended after the URL, and `https://` is automatically added in front.
  engines: [Google] 
)
```
