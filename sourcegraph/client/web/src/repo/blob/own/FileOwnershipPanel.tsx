import * as React from 'react'

import classNames from 'classnames'

import { logger } from '@sourcegraph/common'
import { useQuery } from '@sourcegraph/http-client'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ErrorAlert, LoadingSpinner } from '@sourcegraph/wildcard'

import { useFeatureFlag } from '../../../featureFlags/useFeatureFlag'
import { FetchOwnershipResult, FetchOwnershipVariables } from '../../../graphql-operations'
import { OwnershipAssignPermission } from '../../../rbac/constants'

import { FETCH_OWNERS } from './grapqlQueries'
import { MakeOwnerButton } from './MakeOwnerButton'
import { OwnerList } from './OwnerList'
import { OwnershipPanelProps } from './TreeOwnershipPanel'

import styles from './FileOwnershipPanel.module.scss'

export const FileOwnershipPanel: React.FunctionComponent<OwnershipPanelProps & TelemetryProps> = ({
    repoID,
    revision,
    filePath,
    telemetryService,
}) => {
    React.useEffect(() => {
        telemetryService.log('OwnershipPanelOpened')
    }, [telemetryService])

    const { data, loading, error, refetch } = useQuery<FetchOwnershipResult, FetchOwnershipVariables>(FETCH_OWNERS, {
        variables: {
            repo: repoID,
            revision: revision ?? '',
            currentPath: filePath,
        },
    })
    const [makeOwnerError, setMakeOwnerError] = React.useState<Error | undefined>(undefined)
    const [ownPromotionEnabled] = useFeatureFlag('own-promote')

    if (loading) {
        return (
            <div className={classNames(styles.loaderWrapper, 'text-muted')}>
                <LoadingSpinner inline={true} className="mr-1" /> Loading...
            </div>
        )
    }
    const canAssignOwners = (data?.currentUser?.permissions?.nodes || []).some(
        permission => permission.displayName === OwnershipAssignPermission
    )
    const makeOwnerButton =
        canAssignOwners && ownPromotionEnabled
            ? (userId: string | undefined) => (
                  <MakeOwnerButton
                      onSuccess={refetch}
                      onError={setMakeOwnerError}
                      repoId={repoID}
                      path={filePath}
                      userId={userId}
                  />
              )
            : undefined

    if (error) {
        logger.log(error)
        return (
            <div className={styles.contents}>
                <ErrorAlert error={error} prefix="Error getting ownership data" className="mt-2" />
            </div>
        )
    }

    if (data?.node?.__typename === 'Repository') {
        return (
            <OwnerList
                data={data?.node?.commit?.blob?.ownership}
                isDirectory={false}
                makeOwnerButton={makeOwnerButton}
                makeOwnerError={makeOwnerError}
                repoID={repoID}
                filePath={filePath}
                refetch={refetch}
            />
        )
    }
    return <OwnerList refetch={refetch} filePath={filePath} repoID={repoID} />
}
