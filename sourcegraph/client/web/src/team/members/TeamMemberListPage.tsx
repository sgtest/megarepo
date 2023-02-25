import React, { useCallback, useState } from 'react'

import { mdiPlus } from '@mdi/js'

import { Button, Container, Icon, useDebounce } from '@sourcegraph/wildcard'

import {
    ConnectionContainer,
    ConnectionError,
    ConnectionLoading,
    ConnectionList,
    SummaryContainer,
    ConnectionSummary,
    ShowMoreButton,
    ConnectionForm,
} from '../../components/FilteredConnection/ui'
import { Scalars } from '../../graphql-operations'

import { AddTeamMemberModal } from './AddTeamMemberModal'
import { useTeamMembers } from './backend'
import { TeamMemberNode } from './TeamMemberNode'

interface Props {
    teamID: Scalars['ID']
    teamName: string
    viewerCanAdminister: boolean
}

type OpenModal = 'add-member'

/**
 * A page displaying the team members of a given team.
 */
export const TeamMemberListPage: React.FunctionComponent<React.PropsWithChildren<Props>> = ({
    teamID,
    teamName,
    viewerCanAdminister,
}) => {
    const [openModal, setOpenModal] = useState<OpenModal | undefined>()
    const [searchValue, setSearchValue] = useState('')
    const query = useDebounce(searchValue, 200)

    const { fetchMore, hasNextPage, loading, refetchAll, connection, error } = useTeamMembers(teamName, query)

    const onClickAdd = useCallback<React.MouseEventHandler>(event => {
        event.preventDefault()
        setOpenModal('add-member')
    }, [])
    const closeModal = useCallback(() => {
        setOpenModal(undefined)
    }, [])
    const afterAction = useCallback(() => {
        setOpenModal(undefined)
        refetchAll()
    }, [refetchAll])

    return (
        <>
            <div className="d-flex justify-content-end mb-3">
                <Button disabled={!viewerCanAdminister} onClick={onClickAdd} variant="primary">
                    <Icon aria-hidden={true} svgPath={mdiPlus} /> Add member
                </Button>
            </div>
            <Container className="mb-3">
                <ConnectionContainer>
                    <ConnectionForm
                        inputValue={searchValue}
                        onInputChange={event => setSearchValue(event.target.value)}
                        inputPlaceholder="Search teams"
                    />

                    {error && <ConnectionError errors={[error.message]} />}
                    {loading && !connection && <ConnectionLoading />}
                    <ConnectionList as="ul" className="list-group" aria-label="Team members">
                        {connection?.nodes?.map(node => (
                            <TeamMemberNode
                                key={node.id}
                                node={node}
                                teamID={teamID}
                                teamName={teamName}
                                refetchAll={refetchAll}
                                viewerCanAdminister={viewerCanAdminister}
                            />
                        ))}
                    </ConnectionList>
                    {connection && (
                        <SummaryContainer className="mt-2">
                            <ConnectionSummary
                                first={15}
                                centered={true}
                                connection={connection}
                                noun="member"
                                pluralNoun="members"
                                hasNextPage={hasNextPage}
                            />
                            {hasNextPage && <ShowMoreButton centered={true} onClick={fetchMore} />}
                        </SummaryContainer>
                    )}
                </ConnectionContainer>
            </Container>

            {openModal === 'add-member' && (
                <AddTeamMemberModal onCancel={closeModal} afterAdd={afterAction} teamID={teamID} teamName={teamName} />
            )}
        </>
    )
}
