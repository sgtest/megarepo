import { useCallback, useState } from 'react'

import { mdiPencil } from '@mdi/js'

import { logger } from '@sourcegraph/common'
import { Button, ErrorAlert, Form, H3, Icon, Input, Label, Modal, Text } from '@sourcegraph/wildcard'

import { TEAM_DISPLAY_NAME_MAX_LENGTH } from '..'
import { LoaderButton } from '../../components/LoaderButton'
import { Page } from '../../components/Page'
import { Scalars, TeamAreaTeamFields } from '../../graphql-operations'
import { TeamAvatar } from '../TeamAvatar'

import { useChangeTeamDisplayName } from './backend'
import { TeamHeader } from './TeamHeader'

export interface TeamProfilePageProps {
    /** The team that is the subject of the page. */
    team: TeamAreaTeamFields

    /** Called when the team is updated and must be reloaded. */
    onTeamUpdate: () => void
}

type OpenModal = 'edit-display-name'

export const TeamProfilePage: React.FunctionComponent<TeamProfilePageProps> = ({ team, onTeamUpdate }) => {
    const [openModal, setOpenModal] = useState<OpenModal | undefined>()

    const onEditDisplayName = useCallback<React.MouseEventHandler>(event => {
        event.preventDefault()
        setOpenModal('edit-display-name')
    }, [])
    const closeModal = useCallback(() => {
        setOpenModal(undefined)
    }, [])
    const afterAction = useCallback(() => {
        setOpenModal(undefined)
        onTeamUpdate()
    }, [onTeamUpdate])

    return (
        <>
            <Page className="mb-3">
                <TeamHeader team={team} className="mb-3" />
                <div className="container">
                    <H3>Team name</H3>
                    <Text>
                        <TeamAvatar team={team} className="mr-1" />
                        {team.name}
                    </Text>
                    <H3>Display Name</H3>
                    <Text className="d-flex align-items-center">
                        {team.displayName && <span>{team.displayName}</span>}
                        {!team.displayName && <span className="text-muted">No display name set</span>}{' '}
                        {team.viewerCanAdminister && (
                            <Button variant="link" onClick={onEditDisplayName} className="ml-2" size="sm">
                                <Icon inline={true} aria-label="Edit team display name" svgPath={mdiPencil} />
                            </Button>
                        )}
                    </Text>
                </div>
            </Page>

            {openModal === 'edit-display-name' && (
                <EditTeamDisplayNameModal
                    onCancel={closeModal}
                    afterEdit={afterAction}
                    teamID={team.id}
                    teamName={team.name}
                    displayName={team.displayName}
                />
            )}
        </>
    )
}

interface EditTeamDisplayNameModalProps {
    teamID: Scalars['ID']
    teamName: string
    displayName: string | null

    onCancel: () => void
    afterEdit: () => void
}

const EditTeamDisplayNameModal: React.FunctionComponent<React.PropsWithChildren<EditTeamDisplayNameModalProps>> = ({
    teamID,
    teamName,
    displayName: currentDisplayName,
    onCancel,
    afterEdit,
}) => {
    const labelId = 'editDisplayName'

    const [displayName, setDisplayName] = useState<string>(currentDisplayName ?? '')
    const onDisplayNameChange: React.ChangeEventHandler<HTMLInputElement> = event => {
        setDisplayName(event.currentTarget.value)
    }

    const [editTeam, { loading, error }] = useChangeTeamDisplayName()

    const onSubmit = useCallback<React.FormEventHandler<HTMLFormElement>>(
        async event => {
            event.preventDefault()

            if (!event.currentTarget.checkValidity()) {
                return
            }

            try {
                await editTeam({ variables: { id: teamID, displayName: displayName ?? null } })

                afterEdit()
            } catch (error) {
                // Non-request error. API errors will be available under `error` above.
                logger.error(error)
            }
        },
        [afterEdit, teamID, displayName, editTeam]
    )

    return (
        <Modal onDismiss={onCancel} aria-labelledby={labelId}>
            <H3 id={labelId}>Modify team {teamName} display name</H3>

            {error && <ErrorAlert error={error} />}

            <Form onSubmit={onSubmit}>
                <Label htmlFor="edit-team--displayname" className="mt-2">
                    Display name
                </Label>
                <Input
                    id="edit-team--displayname"
                    placeholder="Engineering Team"
                    maxLength={TEAM_DISPLAY_NAME_MAX_LENGTH}
                    autoCorrect="off"
                    value={displayName}
                    onChange={onDisplayNameChange}
                    disabled={loading}
                />

                <div className="d-flex justify-content-end pt-1">
                    <Button disabled={loading} className="mr-2" onClick={onCancel} outline={true} variant="secondary">
                        Cancel
                    </Button>
                    <LoaderButton
                        type="submit"
                        variant="primary"
                        loading={loading}
                        disabled={loading}
                        alwaysShowLabel={true}
                        label="Save"
                    />
                </div>
            </Form>
        </Modal>
    )
}
