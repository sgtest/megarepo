// @ts-check

/** @type {import('@babel/core').ConfigFunction} */
module.exports = api => {
  api.cache.forever()

  return {
    presets: [
      [
        '@babel/preset-env',
        {
          modules: false,
          useBuiltIns: 'entry',
          corejs: 3,
        },
      ],
      '@babel/preset-typescript',
      '@babel/preset-react',
    ],
    plugins: [
      '@babel/plugin-syntax-dynamic-import',
      'babel-plugin-lodash',

      // Node 12 (released 2019 Apr 23) supports these natively, so we can remove this plugin soon.
      '@babel/plugin-proposal-class-properties',
    ],
  }
}
