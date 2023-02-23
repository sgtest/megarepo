import { FC, ReactElement } from 'react'

import { useQuery } from '@apollo/client'
import { mdiInformationOutline, mdiDelete, mdiPlus } from '@mdi/js'
import classNames from 'classnames'

import { ErrorAlert, Icon, LoadingSpinner, Button, Tooltip, Link } from '@sourcegraph/wildcard'

import { CodeHost, GetCodeHostsResult } from '../../../../../graphql-operations'
import { getCodeHostIcon, getCodeHostKindFromURLParam, getCodeHostName } from '../../helpers'
import { GET_CODE_HOSTS } from '../../queries'

import styles from './CodeHostsNavigation.module.scss'

interface CodeHostsNavigationProps {
    activeConnectionId: string | undefined
    createConnectionType: string | undefined
    className?: string
    onCodeHostDelete: (codeHost: CodeHost) => void
}

export const CodeHostsNavigation: FC<CodeHostsNavigationProps> = props => {
    const { activeConnectionId, createConnectionType, className, onCodeHostDelete } = props

    const { data, loading, error, refetch } = useQuery<GetCodeHostsResult>(GET_CODE_HOSTS, {
        fetchPolicy: 'cache-and-network',
    })

    if (error && !loading) {
        return (
            <div className={className}>
                <ErrorAlert error={error} />
                <Button variant="secondary" outline={true} size="sm" onClick={() => refetch()}>
                    Try fetch again
                </Button>
            </div>
        )
    }

    if (!data || (!data && loading)) {
        return (
            <small className={classNames(className, styles.loadingState)}>
                <LoadingSpinner /> Fetching connected code host...
            </small>
        )
    }

    if (data.externalServices.nodes.length === 0) {
        return (
            <small className={classNames(className, styles.emptyState)}>
                <span>
                    <Icon
                        width={24}
                        height={24}
                        aria-hidden={true}
                        svgPath={mdiInformationOutline}
                        className={styles.emptyStateIcon}
                    />
                </span>
                <span>Choose at least one of the code host providers from the list on the right.</span>
            </small>
        )
    }

    return (
        <ul className={styles.list}>
            {createConnectionType && <CreateCodeHostConnectionCard codeHostType={createConnectionType} />}
            {data.externalServices.nodes.map(codeHost => (
                <li
                    key={codeHost.id}
                    className={classNames(styles.item, { [styles.itemActive]: codeHost.id === activeConnectionId })}
                >
                    <Button
                        as={Link}
                        to={`/setup/remote-repositories/${codeHost.id}/edit`}
                        className={styles.itemButton}
                    >
                        <span>
                            <Icon svgPath={getCodeHostIcon(codeHost.kind)} aria-hidden={true} />
                        </span>
                        <span className={styles.itemDescription}>
                            <span>{codeHost.displayName}</span>
                            <small className={styles.itemDescriptionStatus}>
                                {codeHost.lastSyncAt !== null && <>Synced, {codeHost.repoCount} repositories found</>}
                                {codeHost.lastSyncAt === null && (
                                    <>
                                        <LoadingSpinner />, Syncing{' '}
                                        {codeHost.repoCount > 0 && (
                                            <>, so far {codeHost.repoCount} repositories found</>
                                        )}
                                    </>
                                )}
                            </small>
                        </span>
                    </Button>

                    <Tooltip content="Delete code host connection" placement="right" debounce={0}>
                        <Button className={styles.deleteButton} onClick={() => onCodeHostDelete(codeHost)}>
                            <Icon svgPath={mdiDelete} aria-label="Delete code host connection" />
                        </Button>
                    </Tooltip>
                </li>
            ))}
            <li className={styles.itemWithMoreLink}>
                <Link to="/setup/remote-repositories" className={classNames(styles.moreLink)}>
                    <Icon svgPath={mdiPlus} aria-hidden={true} /> Add more code hosts
                </Link>
            </li>
        </ul>
    )
}

interface CreateCodeHostConnectionCardProps {
    codeHostType: string
}

function CreateCodeHostConnectionCard(props: CreateCodeHostConnectionCardProps): ReactElement {
    const { codeHostType } = props
    const codeHostKind = getCodeHostKindFromURLParam(codeHostType)

    return (
        <li className={classNames(styles.item, styles.itemCreation, styles.itemActive)}>
            <span>
                <Icon svgPath={mdiPlus} aria-hidden={true} />
            </span>
            <span className={styles.itemDescription}>
                <span>
                    Connect <Icon svgPath={getCodeHostIcon(codeHostKind)} aria-hidden={true} />{' '}
                    {getCodeHostName(codeHostKind)}
                </span>
                <small className={styles.itemDescriptionStatus}>
                    New code host will appear in the list as soon as you connect it
                </small>
            </span>
        </li>
    )
}
