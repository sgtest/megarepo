import { map } from 'rxjs/operators'
import { dataOrThrowErrors, gql } from '../../../../../shared/src/graphql/graphql'
import { queryGraphQL, mutateGraphQL } from '../../../backend/graphql'
import { Observable } from 'rxjs'
import {
    ID,
    ICampaign,
    IUpdateCampaignInput,
    ICreateCampaignInput,
    ICampaignPlan,
    ICampaignPlanSpecification,
} from '../../../../../shared/src/graphql/schema'
import { DiffStatFields, FileDiffHunkRangeFields, PreviewFileDiffFields, FileDiffFields } from '../../../backend/diff'

export type CampaignType = 'comby' | 'credentials'

const campaignFragment = gql`
    fragment CampaignFields on Campaign {
        id
        author {
            username
            avatarURL
        }
        changesetCreationStatus {
            completedCount
            pendingCount
            state
            errors
        }
        name
        description
        createdAt
        updatedAt
        closedAt
        url
        __typename
        changesets {
            totalCount
            nodes {
                id
                repository {
                    id
                    name
                    url
                }
                diff {
                    fileDiffs {
                        nodes {
                            ...FileDiffFields
                        }
                        totalCount
                        pageInfo {
                            hasNextPage
                        }
                        diffStat {
                            ...DiffStatFields
                        }
                    }
                }
            }
        }
        plan {
            id
            type
            arguments
        }
        # TODO move to separate query and configure from/to
        changesetCountsOverTime {
            date
            merged
            closed
            openApproved
            openChangesRequested
            openPending
            total
        }
    }

    ${FileDiffFields}

    ${FileDiffHunkRangeFields}

    ${DiffStatFields}
`

const campaignPlanFragment = gql`
    fragment CampaignPlanFields on CampaignPlan {
        id
        type
        arguments
        status {
            completedCount
            pendingCount
            state
            errors
        }
        changesets {
            totalCount
            nodes {
                __typename
                id
                repository {
                    id
                    name
                    url
                }
                diff {
                    fileDiffs {
                        nodes {
                            ...PreviewFileDiffFields
                        }
                        totalCount
                        pageInfo {
                            hasNextPage
                        }
                        diffStat {
                            ...DiffStatFields
                        }
                    }
                }
            }
        }
    }

    ${PreviewFileDiffFields}

    ${FileDiffHunkRangeFields}

    ${DiffStatFields}
`

export async function updateCampaign(update: IUpdateCampaignInput): Promise<ICampaign> {
    const result = await mutateGraphQL(
        gql`
            mutation UpdateCampaign($update: UpdateCampaignInput!) {
                updateCampaign(input: $update) {
                    ...CampaignFields
                }
            }
            ${campaignFragment}
        `,
        { update }
    ).toPromise()
    return dataOrThrowErrors(result).updateCampaign
}

export async function createCampaign(input: ICreateCampaignInput): Promise<ICampaign> {
    const result = await mutateGraphQL(
        gql`
            mutation CreateCampaign($input: CreateCampaignInput!) {
                createCampaign(input: $input) {
                    id
                    url
                }
            }
        `,
        { input }
    ).toPromise()
    return dataOrThrowErrors(result).createCampaign
}

export function previewCampaignPlan(
    specification: ICampaignPlanSpecification,
    wait: boolean = false
): Observable<ICampaignPlan> {
    return mutateGraphQL(
        gql`
            mutation PreviewCampaignPlan($specification: CampaignPlanSpecification!, $wait: Boolean!) {
                previewCampaignPlan(specification: $specification, wait: $wait) {
                    ...CampaignPlanFields
                }
            }
            ${campaignPlanFragment}
        `,
        { specification, wait }
    ).pipe(
        map(dataOrThrowErrors),
        map(mutation => mutation.previewCampaignPlan)
    )
}

export async function retryCampaign(campaignID: ID): Promise<void> {
    const result = await mutateGraphQL(
        gql`
            mutation RetryCampaign($campaign: ID!) {
                retryCampaign(campaign: $campaign) {
                    id
                }
            }
        `,
        { campaign: campaignID }
    ).toPromise()
    dataOrThrowErrors(result)
}

export async function closeCampaign(campaign: ID, closeChangesets = false): Promise<void> {
    const result = await mutateGraphQL(
        gql`
            mutation CloseCampaign($campaign: ID!, $closeChangesets: Boolean!) {
                closeCampaign(campaign: $campaign, closeChangesets: $closeChangesets) {
                    id
                }
            }
        `,
        { campaign, closeChangesets }
    ).toPromise()
    dataOrThrowErrors(result)
}

export async function deleteCampaign(campaign: ID, closeChangesets = false): Promise<void> {
    const result = await mutateGraphQL(
        gql`
            mutation DeleteCampaign($campaign: ID!, $closeChangesets: Boolean!) {
                deleteCampaign(campaign: $campaign, closeChangesets: $closeChangesets) {
                    alwaysNil
                }
            }
        `,
        { campaign, closeChangesets }
    ).toPromise()
    dataOrThrowErrors(result)
}

export const fetchCampaignById = (campaign: ID): Observable<ICampaign | null> =>
    queryGraphQL(
        gql`
            query CampaignByID($campaign: ID!) {
                node(id: $campaign) {
                    __typename
                    ... on Campaign {
                        ...CampaignFields
                    }
                }
            }
            ${campaignFragment}
        `,
        { campaign }
    ).pipe(
        map(dataOrThrowErrors),
        map(({ node }) => {
            if (!node) {
                return null
            }
            if (node.__typename !== 'Campaign') {
                throw new Error(`The given ID is a ${node.__typename}, not a Campaign`)
            }
            return node
        })
    )

export const fetchCampaignPlanById = (campaignPlan: ID): Observable<ICampaignPlan | null> =>
    queryGraphQL(
        gql`
            query CampaignPlanByID($campaignPlan: ID!) {
                node(id: $campaignPlan) {
                    __typename
                    ... on CampaignPlan {
                        ...CampaignPlanFields
                    }
                }
            }
            ${campaignPlanFragment}
        `,
        { campaignPlan }
    ).pipe(
        map(dataOrThrowErrors),
        map(({ node }) => {
            if (!node) {
                return null
            }
            if (node.__typename !== 'CampaignPlan') {
                throw new Error(`The given ID is a ${node.__typename}, not a CampaignPlan`)
            }
            return node
        })
    )
