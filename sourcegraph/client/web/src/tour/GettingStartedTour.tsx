import { type FC, memo } from 'react'

import { useTemporarySetting } from '@sourcegraph/shared/src/settings/temporary'

import type { AuthenticatedUser } from '../auth'
import { withFeatureFlag } from '../featureFlags/withFeatureFlag'

import { Tour, type TourProps } from './components/Tour/Tour'
import { TourInfo } from './components/Tour/TourInfo'
import { withErrorBoundary } from './components/withErrorBoundary'
import { authenticatedExtraTask, useOnboardingTasks } from './data'
import { GettingStartedTourSetup } from './GettingStartedTourSetup'
import { useShowOnboardingSetup } from './hooks'

const GatedTour = withFeatureFlag('end-user-onboarding', Tour)

interface TourWrapperProps
    extends Omit<TourProps, 'useStore' | 'eventPrefix' | 'tasks' | 'id' | 'defaultSnippets' | 'userInfo'> {
    authenticatedUser: AuthenticatedUser | null
}

const TourWrapper: FC<TourWrapperProps> = ({ authenticatedUser, ...props }) => {
    const showOnboardingSetup = useShowOnboardingSetup()
    const [config] = useTemporarySetting('onboarding.userconfig')

    const { loading, error, data } = useOnboardingTasks()
    if (loading || error || !data) {
        return null
    }

    if (authenticatedUser && showOnboardingSetup) {
        return <GettingStartedTourSetup user={authenticatedUser} />
    }

    return (
        <GatedTour
            {...props}
            id="GettingStarted"
            userInfo={config?.userinfo}
            defaultSnippets={data.defaultSnippets}
            tasks={data.tasks}
            extraTask={authenticatedExtraTask}
        />
    )
}

// This needed to be split up into two compontent definitions because
// eslint warns that `useOnboardingTasks` cannot be used inside a callback
// (but the value passed to `withErrorBoundary` really is a component)
const TourWithErrorBoundary = memo(withErrorBoundary(TourWrapper))

export const GettingStartedTour = Object.assign(TourWithErrorBoundary, {
    Info: TourInfo,
})
