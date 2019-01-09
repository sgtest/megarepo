// tslint:disable:no-submodule-imports Avoid loading grammars for unused languages.
import { registerLanguage } from 'highlight.js/lib/highlight'

let registered = false

/**
 * Registers syntax highlighters for commonly used languages.
 *
 * This function must be called exactly once. A function is used instead of having the registerLanguage calls be
 * side effects of importing this module to prevent this module from being omitted from production builds due to
 * tree-shaking.
 */
export function registerHighlightContributions(): void {
    if (registered) {
        // Don't double-register these. (There is no way to unregister them.)
        return
    }
    registered = true
    registerLanguage('go', require('highlight.js/lib/languages/go'))
    registerLanguage('javascript', require('highlight.js/lib/languages/javascript'))
    registerLanguage('typescript', require('highlight.js/lib/languages/typescript'))
    registerLanguage('java', require('highlight.js/lib/languages/java'))
    registerLanguage('python', require('highlight.js/lib/languages/python'))
    registerLanguage('php', require('highlight.js/lib/languages/php'))
    registerLanguage('bash', require('highlight.js/lib/languages/bash'))
    registerLanguage('clojure', require('highlight.js/lib/languages/clojure'))
    registerLanguage('cpp', require('highlight.js/lib/languages/cpp'))
    registerLanguage('cs', require('highlight.js/lib/languages/cs'))
    registerLanguage('css', require('highlight.js/lib/languages/css'))
    registerLanguage('dockerfile', require('highlight.js/lib/languages/dockerfile'))
    registerLanguage('elixir', require('highlight.js/lib/languages/elixir'))
    registerLanguage('haskell', require('highlight.js/lib/languages/haskell'))
    registerLanguage('html', require('highlight.js/lib/languages/xml'))
    registerLanguage('lua', require('highlight.js/lib/languages/lua'))
    registerLanguage('ocaml', require('highlight.js/lib/languages/ocaml'))
    registerLanguage('r', require('highlight.js/lib/languages/r'))
    registerLanguage('ruby', require('highlight.js/lib/languages/ruby'))
    registerLanguage('rust', require('highlight.js/lib/languages/rust'))
    registerLanguage('swift', require('highlight.js/lib/languages/swift'))
    registerLanguage('markdown', require('highlight.js/lib/languages/markdown'))
    registerLanguage('diff', require('highlight.js/lib/languages/diff'))
    registerLanguage('json', require('highlight.js/lib/languages/json'))
    registerLanguage('yaml', require('highlight.js/lib/languages/yaml'))
}
