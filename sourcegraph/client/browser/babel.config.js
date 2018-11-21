// @ts-check

/** @type {import('@babel/core').TransformOptions} */
const config = {
  plugins: ['babel-plugin-lodash'],
  presets: [
    [
      '@babel/preset-env',
      {
        modules: false,
        useBuiltIns: 'entry',
      },
    ],
  ],
}

module.exports = config
