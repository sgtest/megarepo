import { map } from 'rxjs/operators'
import { dataOrThrowErrors, gql } from '../../../../../shared/src/graphql/graphql'
import { Observable } from 'rxjs'
import { diffStatFields, fileDiffFields } from '../../../backend/diff'
import {
    CampaignFields,
    CampaignChangesetsVariables,
    CampaignChangesetsResult,
    ExternalChangesetFileDiffsResult,
    ExternalChangesetFileDiffsVariables,
    ExternalChangesetFileDiffsFields,
    SyncChangesetResult,
    SyncChangesetVariables,
    Scalars,
    ChangesetCountsOverTimeVariables,
    ChangesetCountsOverTimeFields,
    ChangesetCountsOverTimeResult,
    DeleteCampaignResult,
    DeleteCampaignVariables,
    CampaignByNamespaceResult,
    CampaignByNamespaceVariables,
} from '../../../graphql-operations'
import { requestGraphQL } from '../../../backend/graphql'

const changesetStatsFragment = gql`
    fragment ChangesetStatsFields on ChangesetConnectionStats {
        total
        closed
        deleted
        merged
        open
        unpublished
    }
`

const campaignFragment = gql`
    fragment CampaignFields on Campaign {
        __typename
        id
        url
        name
        namespace {
            namespaceName
            url
        }
        description

        createdAt
        initialApplier {
            username
            url
        }

        lastAppliedAt
        lastApplier {
            username
            url
        }

        updatedAt
        closedAt
        viewerCanAdminister

        changesets {
            stats {
                ...ChangesetStatsFields
            }
        }

        diffStat {
            ...DiffStatFields
        }

        currentSpec {
            originalInput
        }
    }

    ${diffStatFields}

    ${changesetStatsFragment}
`

const changesetLabelFragment = gql`
    fragment ChangesetLabelFields on ChangesetLabel {
        color
        description
        text
    }
`

export const fetchCampaignByNamespace = (
    namespaceID: Scalars['ID'],
    campaign: CampaignFields['name']
): Observable<CampaignFields | null> =>
    requestGraphQL<CampaignByNamespaceResult, CampaignByNamespaceVariables>(
        gql`
            query CampaignByNamespace($namespaceID: ID!, $campaign: String!) {
                campaign(namespace: $namespaceID, name: $campaign) {
                    ...CampaignFields
                }
            }
            ${campaignFragment}
        `,
        { namespaceID, campaign }
    ).pipe(
        map(dataOrThrowErrors),
        map(({ campaign }) => {
            if (!campaign) {
                return null
            }
            return campaign
        })
    )

export const hiddenExternalChangesetFieldsFragment = gql`
    fragment HiddenExternalChangesetFields on HiddenExternalChangeset {
        __typename
        id
        createdAt
        updatedAt
        nextSyncAt
        externalState
        publicationState
        reconcilerState
    }
`
export const externalChangesetFieldsFragment = gql`
    fragment ExternalChangesetFields on ExternalChangeset {
        __typename
        id
        title
        body
        publicationState
        reconcilerState
        externalState
        reviewState
        checkState
        error
        labels {
            ...ChangesetLabelFields
        }
        repository {
            id
            name
            url
        }
        externalURL {
            url
        }
        externalID
        diffStat {
            ...DiffStatFields
        }
        createdAt
        updatedAt
        nextSyncAt
        currentSpec {
            id
        }
    }

    ${diffStatFields}

    ${changesetLabelFragment}
`

export const changesetFieldsFragment = gql`
    fragment ChangesetFields on Changeset {
        __typename
        ... on HiddenExternalChangeset {
            ...HiddenExternalChangesetFields
        }
        ... on ExternalChangeset {
            ...ExternalChangesetFields
        }
    }

    ${hiddenExternalChangesetFieldsFragment}

    ${externalChangesetFieldsFragment}
`

export const queryChangesets = ({
    campaign,
    first,
    after,
    externalState,
    reviewState,
    checkState,
    publicationState,
    reconcilerState,
    onlyPublishedByThisCampaign,
}: CampaignChangesetsVariables): Observable<
    (CampaignChangesetsResult['node'] & { __typename: 'Campaign' })['changesets']
> =>
    requestGraphQL<CampaignChangesetsResult, CampaignChangesetsVariables>(
        gql`
            query CampaignChangesets(
                $campaign: ID!
                $first: Int
                $after: String
                $externalState: ChangesetExternalState
                $reviewState: ChangesetReviewState
                $checkState: ChangesetCheckState
                $publicationState: ChangesetPublicationState
                $reconcilerState: [ChangesetReconcilerState!]
                $onlyPublishedByThisCampaign: Boolean
            ) {
                node(id: $campaign) {
                    __typename
                    ... on Campaign {
                        changesets(
                            first: $first
                            after: $after
                            externalState: $externalState
                            publicationState: $publicationState
                            reconcilerState: $reconcilerState
                            reviewState: $reviewState
                            checkState: $checkState
                            onlyPublishedByThisCampaign: $onlyPublishedByThisCampaign
                        ) {
                            totalCount
                            pageInfo {
                                endCursor
                                hasNextPage
                            }
                            nodes {
                                ...ChangesetFields
                            }
                        }
                    }
                }
            }

            ${changesetFieldsFragment}
        `,
        {
            campaign,
            first,
            after,
            externalState,
            reviewState,
            checkState,
            publicationState,
            reconcilerState,
            onlyPublishedByThisCampaign,
        }
    ).pipe(
        map(dataOrThrowErrors),
        map(({ node }) => {
            if (!node) {
                throw new Error(`Campaign with ID ${campaign} does not exist`)
            }
            if (node.__typename !== 'Campaign') {
                throw new Error(`The given ID is a ${node.__typename}, not a Campaign`)
            }
            return node.changesets
        })
    )

export async function syncChangeset(changeset: Scalars['ID']): Promise<void> {
    const result = await requestGraphQL<SyncChangesetResult, SyncChangesetVariables>(
        gql`
            mutation SyncChangeset($changeset: ID!) {
                syncChangeset(changeset: $changeset) {
                    alwaysNil
                }
            }
        `,
        { changeset }
    ).toPromise()
    dataOrThrowErrors(result)
}

// Because thats the name in the API:
// eslint-disable-next-line unicorn/prevent-abbreviations
export const gitRefSpecFields = gql`
    fragment GitRefSpecFields on GitRevSpec {
        __typename
        ... on GitObject {
            oid
        }
        ... on GitRef {
            target {
                oid
            }
        }
        ... on GitRevSpecExpr {
            object {
                oid
            }
        }
    }
`

export const externalChangesetFileDiffsFields = gql`
    fragment ExternalChangesetFileDiffsFields on ExternalChangeset {
        diff {
            __typename
            ... on RepositoryComparison {
                range {
                    base {
                        ...GitRefSpecFields
                    }
                    head {
                        ...GitRefSpecFields
                    }
                }
                fileDiffs(first: $first, after: $after) {
                    nodes {
                        ...FileDiffFields
                    }
                    totalCount
                    pageInfo {
                        hasNextPage
                        endCursor
                    }
                }
            }
            ... on PreviewRepositoryComparison {
                fileDiffs(first: $first, after: $after) {
                    nodes {
                        ...FileDiffFields
                    }
                    totalCount
                    pageInfo {
                        hasNextPage
                        endCursor
                    }
                }
            }
        }
    }

    ${fileDiffFields}

    ${gitRefSpecFields}
`

export const queryExternalChangesetWithFileDiffs = ({
    externalChangeset,
    first,
    after,
    isLightTheme,
}: ExternalChangesetFileDiffsVariables): Observable<ExternalChangesetFileDiffsFields> =>
    requestGraphQL<ExternalChangesetFileDiffsResult, ExternalChangesetFileDiffsVariables>(
        gql`
            query ExternalChangesetFileDiffs(
                $externalChangeset: ID!
                $first: Int
                $after: String
                $isLightTheme: Boolean!
            ) {
                node(id: $externalChangeset) {
                    __typename
                    ...ExternalChangesetFileDiffsFields
                }
            }

            ${externalChangesetFileDiffsFields}
        `,
        { externalChangeset, first, after, isLightTheme }
    ).pipe(
        map(dataOrThrowErrors),
        map(({ node }) => {
            if (!node) {
                throw new Error(`Changeset with ID ${externalChangeset} does not exist`)
            }
            if (node.__typename !== 'ExternalChangeset') {
                throw new Error(`The given ID is a ${node.__typename}, not an ExternalChangeset`)
            }
            return node
        })
    )

const changesetCountsOverTimeFragment = gql`
    fragment ChangesetCountsOverTimeFields on ChangesetCounts {
        date
        merged
        closed
        openApproved
        openChangesRequested
        openPending
        total
    }
`

export const queryChangesetCountsOverTime = ({
    campaign,
}: ChangesetCountsOverTimeVariables): Observable<ChangesetCountsOverTimeFields[]> =>
    requestGraphQL<ChangesetCountsOverTimeResult, ChangesetCountsOverTimeVariables>(
        gql`
            query ChangesetCountsOverTime($campaign: ID!) {
                node(id: $campaign) {
                    __typename
                    ... on Campaign {
                        changesetCountsOverTime {
                            ...ChangesetCountsOverTimeFields
                        }
                    }
                }
            }

            ${changesetCountsOverTimeFragment}
        `,
        { campaign }
    ).pipe(
        map(dataOrThrowErrors),
        map(({ node }) => {
            if (!node) {
                throw new Error(`Campaign with ID ${campaign} does not exist`)
            }
            if (node.__typename !== 'Campaign') {
                throw new Error(`The given ID is a ${node.__typename}, not a Campaign`)
            }
            return node.changesetCountsOverTime
        })
    )

export async function deleteCampaign(campaign: Scalars['ID']): Promise<void> {
    const result = await requestGraphQL<DeleteCampaignResult, DeleteCampaignVariables>(
        gql`
            mutation DeleteCampaign($campaign: ID!) {
                deleteCampaign(campaign: $campaign) {
                    alwaysNil
                }
            }
        `,
        { campaign }
    ).toPromise()
    dataOrThrowErrors(result)
}
