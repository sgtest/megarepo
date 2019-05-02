// We want to polyfill first.
// prettier-ignore
import '../../config/polyfill'

import * as React from 'react'
import { render } from 'react-dom'
import { noop, Subscription } from 'rxjs'
import storage from '../../browser/storage'
import { featureFlagDefaults, FeatureFlags } from '../../browser/types'
import { OptionsMenuProps } from '../../libs/options/Menu'
import { OptionsContainer, OptionsContainerProps } from '../../libs/options/OptionsContainer'
import { initSentry } from '../../libs/sentry'
import { fetchSite } from '../../shared/backend/server'
import { featureFlags } from '../../shared/util/featureFlags'
import { assertEnv } from '../envAssertion'

assertEnv('OPTIONS')

initSentry('options')

type State = Pick<
    FeatureFlags,
    'allowErrorReporting' | 'experimentalLinkPreviews' | 'experimentalTextFieldCompletion'
> & { sourcegraphURL: string | null }

const keyIsFeatureFlag = (key: string): key is keyof FeatureFlags =>
    !!Object.keys(featureFlagDefaults).find(k => key === k)

const toggleFeatureFlag = (key: string) => {
    if (keyIsFeatureFlag(key)) {
        featureFlags
            .toggle(key)
            .then(noop)
            .catch(noop)
    }
}

const fetchCurrentTabStatus = async (): Promise<OptionsMenuProps['currentTabStatus']> => {
    const tabs = await browser.tabs.query({ active: true, currentWindow: true })
    if (tabs.length > 1) {
        throw new Error('Querying for the currently active tab returned more than one result')
    }
    const { url } = tabs[0]
    if (!url) {
        throw new Error('Currently active tab has no URL')
    }
    const { host, protocol } = new URL(url)
    const hasPermissions = await browser.permissions.contains({
        origins: [`${protocol}//${host}/*`],
    })
    return { host, protocol, hasPermissions }
}
class Options extends React.Component<{}, State> {
    public state: State = {
        sourcegraphURL: null,
        allowErrorReporting: false,
        experimentalLinkPreviews: false,
        experimentalTextFieldCompletion: false,
    }

    private subscriptions = new Subscription()

    public componentDidMount(): void {
        this.subscriptions.add(
            storage
                .observeSync('featureFlags')
                .subscribe(({ allowErrorReporting, experimentalLinkPreviews, experimentalTextFieldCompletion }) => {
                    this.setState({ allowErrorReporting, experimentalLinkPreviews, experimentalTextFieldCompletion })
                })
        )

        this.subscriptions.add(
            storage.observeSync('sourcegraphURL').subscribe(sourcegraphURL => {
                this.setState({ sourcegraphURL })
            })
        )
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): React.ReactNode {
        if (this.state.sourcegraphURL === null) {
            return null
        }

        const props: OptionsContainerProps = {
            sourcegraphURL: this.state.sourcegraphURL,

            ensureValidSite: fetchSite,
            fetchCurrentTabStatus,
            hasPermissions: url =>
                browser.permissions.contains({
                    origins: [`${url}/*`],
                }),
            requestPermissions: url =>
                browser.permissions.request({
                    origins: [`${url}/*`],
                }),

            setSourcegraphURL: (url: string) => {
                storage.setSync({ sourcegraphURL: url })
            },

            toggleFeatureFlag,
            featureFlags: [
                { key: 'allowErrorReporting', value: this.state.allowErrorReporting },
                { key: 'experimentalLinkPreviews', value: this.state.experimentalLinkPreviews },
                { key: 'experimentalTextFieldCompletion', value: this.state.experimentalTextFieldCompletion },
            ],
        }

        return <OptionsContainer {...props} />
    }
}

const inject = async () => {
    const injectDOM = document.createElement('div')
    injectDOM.className = 'sourcegraph-options-menu options'
    document.body.appendChild(injectDOM)

    render(<Options />, injectDOM)
}

document.addEventListener('DOMContentLoaded', async () => {
    await inject()
})
