// @ts-check

const baseConfig = require('../../.eslintrc')
module.exports = {
  extends: '../../.eslintrc.js',
  parserOptions: {
    ...baseConfig.parserOptions,
    project: [__dirname + '/tsconfig.json'],
  },
  overrides: baseConfig.overrides,
  rules: {
    'no-console': 'off',
    'id-length': 'off',
    'no-restricted-imports': [
      'error',
      {
        paths: ['!highlight.js'],
      },
    ],
  },
}
