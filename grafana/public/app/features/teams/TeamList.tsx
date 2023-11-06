import React, { useEffect, useMemo, useState } from 'react';
import { connect, ConnectedProps } from 'react-redux';

import {
  Avatar,
  CellProps,
  Column,
  DeleteButton,
  FilterInput,
  Icon,
  InlineField,
  InteractiveTable,
  LinkButton,
  Pagination,
  Stack,
  Tooltip,
} from '@grafana/ui';
import EmptyListCTA from 'app/core/components/EmptyListCTA/EmptyListCTA';
import { Page } from 'app/core/components/Page/Page';
import { fetchRoleOptions } from 'app/core/components/RolePicker/api';
import { contextSrv } from 'app/core/services/context_srv';
import { AccessControlAction, Role, StoreState, Team } from 'app/types';

import { TeamRolePicker } from '../../core/components/RolePicker/TeamRolePicker';

import { deleteTeam, loadTeams, changePage, changeQuery, changeSort } from './state/actions';

type Cell<T extends keyof Team = keyof Team> = CellProps<Team, Team[T]>;
export interface OwnProps {}

export interface State {
  roleOptions: Role[];
}

export const TeamList = ({
  teams,
  query,
  noTeams,
  hasFetched,
  loadTeams,
  deleteTeam,
  changeQuery,
  totalPages,
  page,
  rolesLoading,
  changePage,
  changeSort,
}: Props) => {
  const [roleOptions, setRoleOptions] = useState<Role[]>([]);

  useEffect(() => {
    loadTeams(true);
  }, [loadTeams]);

  useEffect(() => {
    if (contextSrv.licensedAccessControlEnabled() && contextSrv.hasPermission(AccessControlAction.ActionRolesList)) {
      fetchRoleOptions().then((roles) => setRoleOptions(roles));
    }
  }, []);

  const canCreate = contextSrv.hasPermission(AccessControlAction.ActionTeamsCreate);
  const displayRolePicker = shouldDisplayRolePicker();

  const columns: Array<Column<Team>> = useMemo(
    () => [
      {
        id: 'avatarUrl',
        header: '',
        cell: ({ cell: { value } }: Cell<'avatarUrl'>) => value && <Avatar src={value} alt="User avatar" />,
      },
      {
        id: 'name',
        header: 'Name',
        cell: ({ cell: { value } }: Cell<'name'>) => value,
        sortType: 'string',
      },
      {
        id: 'email',
        header: 'Email',
        cell: ({ cell: { value } }: Cell<'email'>) => value,
        sortType: 'string',
      },
      {
        id: 'memberCount',
        header: 'Members',
        cell: ({ cell: { value } }: Cell<'memberCount'>) => value,
        sortType: 'number',
      },
      ...(displayRolePicker
        ? [
            {
              id: 'role',
              header: 'Role',
              cell: ({ cell: { value }, row: { original } }: Cell<'memberCount'>) => {
                const canSeeTeamRoles = contextSrv.hasPermissionInMetadata(
                  AccessControlAction.ActionTeamsRolesList,
                  original
                );
                return (
                  canSeeTeamRoles && (
                    <TeamRolePicker
                      teamId={original.id}
                      roles={original.roles || []}
                      isLoading={rolesLoading}
                      roleOptions={roleOptions}
                      width={40}
                    />
                  )
                );
              },
            },
          ]
        : []),
      {
        id: 'edit',
        header: '',
        cell: ({ row: { original } }: Cell) => {
          const canReadTeam = contextSrv.hasPermissionInMetadata(AccessControlAction.ActionTeamsRead, original);
          return canReadTeam ? (
            <a href={`org/teams/edit/${original.id}`} aria-label={`Edit team ${original.name}`}>
              <Tooltip content={'Edit team'}>
                <Icon name={'pen'} />
              </Tooltip>
            </a>
          ) : null;
        },
      },
      {
        id: 'delete',
        header: '',
        cell: ({ row: { original } }: Cell) => {
          const canDelete = contextSrv.hasPermissionInMetadata(AccessControlAction.ActionTeamsDelete, original);

          return (
            <DeleteButton
              aria-label={`Delete team ${original.name}`}
              size="sm"
              disabled={!canDelete}
              onConfirm={() => deleteTeam(original.id)}
            />
          );
        },
      },
    ],
    [displayRolePicker, rolesLoading, roleOptions, deleteTeam]
  );

  return (
    <Page navId="teams">
      <Page.Contents isLoading={!hasFetched}>
        {noTeams ? (
          <EmptyListCTA
            title="You haven't created any teams yet."
            buttonIcon="users-alt"
            buttonLink="org/teams/new"
            buttonTitle=" New team"
            buttonDisabled={!contextSrv.hasPermission(AccessControlAction.ActionTeamsCreate)}
            proTip="Assign folder and dashboard permissions to teams instead of users to ease administration."
            proTipLink=""
            proTipLinkTitle=""
            proTipTarget="_blank"
          />
        ) : (
          <>
            <div className="page-action-bar">
              <InlineField grow>
                <FilterInput placeholder="Search teams" value={query} onChange={changeQuery} />
              </InlineField>

              <LinkButton href={canCreate ? 'org/teams/new' : '#'} disabled={!canCreate}>
                New Team
              </LinkButton>
            </div>
            <Stack gap={2}>
              <InteractiveTable
                columns={columns}
                data={teams}
                getRowId={(team) => String(team.id)}
                fetchData={changeSort}
              />
              <Stack justifyContent="flex-end">
                <Pagination hideWhenSinglePage currentPage={page} numberOfPages={totalPages} onNavigate={changePage} />
              </Stack>
            </Stack>
          </>
        )}
      </Page.Contents>
    </Page>
  );
};

function shouldDisplayRolePicker(): boolean {
  return (
    contextSrv.licensedAccessControlEnabled() &&
    contextSrv.hasPermission(AccessControlAction.ActionTeamsRolesList) &&
    contextSrv.hasPermission(AccessControlAction.ActionRolesList)
  );
}

function mapStateToProps(state: StoreState) {
  return {
    teams: state.teams.teams,
    query: state.teams.query,
    perPage: state.teams.perPage,
    page: state.teams.page,
    noTeams: state.teams.noTeams,
    totalPages: state.teams.totalPages,
    hasFetched: state.teams.hasFetched,
    rolesLoading: state.teams.rolesLoading,
  };
}

const mapDispatchToProps = {
  loadTeams,
  deleteTeam,
  changePage,
  changeQuery,
  changeSort,
};

const connector = connect(mapStateToProps, mapDispatchToProps);
export type Props = OwnProps & ConnectedProps<typeof connector>;
export default connector(TeamList);
