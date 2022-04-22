// This is the entry point for the web app

// Order is important here
// Don't remove the empty lines between these imports

import '@sourcegraph/shared/src/polyfills'

import './monitoring/initMonitoring'

import { render } from 'react-dom'

import { OpenSourceWebApp } from './OpenSourceWebApp'

// It's important to have a root component in a separate file to create a react-refresh boundary and avoid page reload.
// https://github.com/pmmmwh/react-refresh-webpack-plugin/blob/main/docs/TROUBLESHOOTING.md#edits-always-lead-to-full-reload
window.addEventListener('DOMContentLoaded', () => {
    render(<OpenSourceWebApp />, document.querySelector('#root'))
})
