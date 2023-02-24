import { FC, HTMLAttributes, useState } from 'react'

import classNames from 'classnames'
import { Routes, Route, matchPath, useLocation } from 'react-router-dom'

import { Container, Text } from '@sourcegraph/wildcard'

import { FooterWidget, CustomNextButton } from '../setup-steps'

import { CodeHostDeleteModal, CodeHostToDelete } from './components/code-host-delete-modal'
import { CodeHostsPicker } from './components/code-host-picker'
import { CodeHostCreation, CodeHostEdit } from './components/code-hosts'
import { CodeHostsNavigation } from './components/navigation'

import styles from './RemoteRepositoriesStep.module.scss'

interface RemoteRepositoriesStepProps extends HTMLAttributes<HTMLDivElement> {}

export const RemoteRepositoriesStep: FC<RemoteRepositoriesStepProps> = props => {
    const { className, ...attributes } = props

    const location = useLocation()
    const [codeHostToDelete, setCodeHostToDelete] = useState<CodeHostToDelete | null>(null)

    const editConnectionRouteMatch = matchPath('/setup/remote-repositories/:codehostId/edit', location.pathname)
    const newConnectionRouteMatch = matchPath('/setup/remote-repositories/:codeHostType/create', location.pathname)

    return (
        <div {...attributes} className={classNames(className, styles.root)}>
            <Text size="small" className="mb-2">
                Connect remote code hosts where your source code lives.
            </Text>

            <section className={styles.content}>
                <Container className={styles.contentNavigation}>
                    <CodeHostsNavigation
                        activeConnectionId={editConnectionRouteMatch?.params?.codehostId}
                        createConnectionType={newConnectionRouteMatch?.params?.codeHostType}
                        className={styles.navigation}
                        onCodeHostDelete={setCodeHostToDelete}
                    />
                </Container>

                <Container className={styles.contentMain}>
                    <Routes>
                        <Route index={true} element={<CodeHostsPicker />} />
                        <Route path=":codeHostType/create" element={<CodeHostCreation />} />
                        <Route
                            path=":codehostId/edit"
                            element={<CodeHostEdit onCodeHostDelete={setCodeHostToDelete} />}
                        />
                    </Routes>
                </Container>
            </section>

            <FooterWidget>Hello custom content in the footer</FooterWidget>
            <CustomNextButton label="Custom next step label" disabled={true} />

            {codeHostToDelete && (
                <CodeHostDeleteModal codeHost={codeHostToDelete} onDismiss={() => setCodeHostToDelete(null)} />
            )}
        </div>
    )
}
