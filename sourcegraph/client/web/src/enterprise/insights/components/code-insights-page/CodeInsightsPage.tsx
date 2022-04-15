import React from 'react'

import { Page } from '../../../../components/Page'
import { useUiFeatures } from '../../hooks/use-ui-features'

import { CodeInsightsLimitAccessBanner } from './limit-access-banner/CodeInsightsLimitAccessBanner'

interface CodeInsightsPageProps extends React.HTMLAttributes<HTMLDivElement> {}

/**
 * Shared common component for creation a typical code insights pages. Contains common styles
 * and demo mode banner in order to render it across all pages.
 */
export const CodeInsightsPage: React.FunctionComponent<CodeInsightsPageProps> = props => {
    const { licensed } = useUiFeatures()

    return (
        <Page {...props}>
            {!licensed && <CodeInsightsLimitAccessBanner className="mb-4" />}
            {props.children}
        </Page>
    )
}
