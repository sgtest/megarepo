import React, { useEffect } from 'react'

import { useLocation } from 'react-router'

import { useTemporarySetting } from '@sourcegraph/shared/src/settings/temporary/useTemporarySetting'

/**
 * This component uses UTM parameters to detect incoming traffic from our IDE extensions (VS Code
 * and JetBrains) and updates a temporary setting whenever these are found.
 */
export const IdeExtensionTracker: React.FunctionComponent<React.PropsWithChildren<unknown>> = () => {
    const location = useLocation()

    const [, setLastVSCodeDetection] = useTemporarySetting('integrations.vscode.lastDetectionTimestamp', 0)
    const [, setLastJetBrainsDetection] = useTemporarySetting('integrations.jetbrains.lastDetectionTimestamp', 0)

    useEffect(() => {
        const parameters = new URLSearchParams(location.search)
        const utmProductName = parameters.get('utm_product_name')
        const utmMedium = parameters.get('utm_medium')
        const utmSource = parameters.get('utm_source')

        if (utmProductName === 'IntelliJ IDEA') {
            setLastJetBrainsDetection(Date.now())
        } else if (utmMedium === 'VSCODE' || utmSource?.toLowerCase().startsWith('vscode')) {
            setLastVSCodeDetection(Date.now())
        }

        // We only want to capture the IDE UTM parameters on the first page load. In order to avoid
        // rerunning the effect whenever location change, we skip it from the dependency array.
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [setLastJetBrainsDetection, setLastVSCodeDetection])

    return null
}
