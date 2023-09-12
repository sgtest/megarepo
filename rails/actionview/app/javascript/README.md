# Ruby on Rails unobtrusive scripting adapter

This unobtrusive scripting support file is developed for the Ruby on Rails framework, but is not strictly tied to any specific backend. You can drop this into any application to:

- force confirmation dialogs for various actions;
- make non-GET requests from hyperlinks;
- make forms or hyperlinks submit data asynchronously with Ajax;
- have submit buttons become automatically disabled on form submit to prevent double-clicking.

These features are achieved by adding certain [`data` attributes][data] to your HTML markup. Documentation about the various supported `data` attributes is [available here][ujsdocs]. In Rails, they are added by the framework's template helpers.

## Optional prerequisites

Note that the `data` attributes this library adds are a feature of HTML5. If you're not targeting HTML5, these attributes may make your HTML to fail [validation][validator]. However, this shouldn't create any issues for web browsers or other user agents.

## Installation

### Bun
    bun add @rails/ujs

### npm

    npm install @rails/ujs --save

### Yarn

    yarn add @rails/ujs

Ensure that `.yarnclean` does not include `assets` if you use [yarn autoclean](https://yarnpkg.com/lang/en/docs/cli/autoclean/).

## Usage

### Asset pipeline

In a conventional Rails application that uses the asset pipeline, require `rails-ujs` in your `application.js` manifest:

```javascript
//= require rails-ujs
```

### ES2015+

If you're using a JavaScript bundler, add the following to your main JS file:

```javascript
import Rails from "@rails/ujs"
Rails.start()
```

## How to run tests

Run `bundle exec rake ujs:server` first, and then run the web tests by visiting http://localhost:4567 in your browser.

## License

rails-ujs is released under the [MIT License](MIT-LICENSE).

[data]: https://www.w3.org/TR/html5/dom.html#embedding-custom-non-visible-data-with-the-data-attributes "Embedding custom non-visible data with the data-* attributes"
[validator]: https://validator.w3.org/
[csrf]: https://api.rubyonrails.org/classes/ActionController/RequestForgeryProtection.html
[ujsdocs]: https://github.com/rails/jquery-ujs/wiki
