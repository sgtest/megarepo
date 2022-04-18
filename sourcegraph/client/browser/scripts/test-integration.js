const shelljs = require('shelljs')

const { decompressRecordings, deleteRecordings } = require('./utils')

// eslint-disable-next-line no-void
void (async () => {
  await decompressRecordings()
  shelljs.exec('POLLYJS_MODE=replay yarn percy exec -- yarn run-integration', async code => {
    await deleteRecordings()
    process.exit(code)
  })
})()
