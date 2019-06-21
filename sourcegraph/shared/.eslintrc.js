module.exports = {
  rules: {
    'no-restricted-imports': [
      'error',
      {
        paths: [
          {
            name: 'react-router-dom',
            importNames: ['Link'],
            message:
              "Use the src/shared/components/Link component instead of react-router-dom's Link. Reason: Shared code runs on platforms that don't use react-router (such as in the browser extension).",
          },
        ],
      },
    ],
  },
}
