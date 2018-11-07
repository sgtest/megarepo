export type ID = string

export interface IGraphQLResponseRoot {
    data?: IQuery | IMutation
    errors?: IGraphQLResponseError[]
}

export interface IGraphQLResponseError {
    /** Required for all errors */
    message: string
    locations?: IGraphQLResponseErrorLocation[]
    /** 7.2.2 says 'GraphQL servers may provide additional entries to error' */
    [propName: string]: any
}

export interface IGraphQLResponseErrorLocation {
    line: number
    column: number
}

export interface IQuery {
    __typename: 'Query'

    /**
     * @deprecated "this will be removed."
     */
    root: IQuery
    node: Node | null
    repository: IRepository | null
    repositories: IRepositoryConnection
    phabricatorRepo: IPhabricatorRepo | null
    currentUser: IUser | null
    user: IUser | null
    users: IUserConnection
    organization: IOrg | null
    organizations: IOrgConnection
    discussionThreads: IDiscussionThreadConnection
    discussionComments: IDiscussionCommentConnection
    renderMarkdown: string
    configurationSubject: ConfigurationSubject | null
    viewerConfiguration: IConfigurationCascade
    clientConfiguration: IClientConfigurationDetails
    search: ISearch | null
    savedQueries: ISavedQuery[]
    repoGroups: IRepoGroup[]
    site: ISite
    surveyResponses: ISurveyResponseConnection
    extensionRegistry: IExtensionRegistry
    docSitePage: IDocSitePage | null
    dotcom: IDotcomQuery
}

export interface INodeOnQueryArguments {
    id: ID
}

export interface IRepositoryOnQueryArguments {
    name?: string | null
    uri?: string | null
}

export interface IRepositoriesOnQueryArguments {
    first?: number | null
    query?: string | null
    names?: string[] | null

    /**
     * @default true
     */
    enabled?: boolean | null

    /**
     * @default false
     */
    disabled?: boolean | null

    /**
     * @default true
     */
    cloned?: boolean | null

    /**
     * @default true
     */
    cloneInProgress?: boolean | null

    /**
     * @default true
     */
    notCloned?: boolean | null

    /**
     * @default true
     */
    indexed?: boolean | null

    /**
     * @default true
     */
    notIndexed?: boolean | null

    /**
     * @default false
     */
    ciIndexed?: boolean | null

    /**
     * @default false
     */
    notCIIndexed?: boolean | null

    /**
     * @default "REPO_URI"
     */
    orderBy?: RepoOrderBy | null

    /**
     * @default false
     */
    descending?: boolean | null
}

export interface IPhabricatorRepoOnQueryArguments {
    name?: string | null
    uri?: string | null
}

export interface IUserOnQueryArguments {
    username: string
}

export interface IUsersOnQueryArguments {
    first?: number | null
    query?: string | null
    tag?: string | null
    activePeriod?: UserActivePeriod | null
}

export interface IOrganizationOnQueryArguments {
    name: string
}

export interface IOrganizationsOnQueryArguments {
    first?: number | null
    query?: string | null
}

export interface IDiscussionThreadsOnQueryArguments {
    first?: number | null
    query?: string | null
    threadID?: ID | null
    authorUserID?: ID | null
    targetRepositoryID?: ID | null
    targetRepositoryName?: string | null
    targetRepositoryGitCloneURL?: string | null
    targetRepositoryPath?: string | null
}

export interface IDiscussionCommentsOnQueryArguments {
    first?: number | null
    authorUserID?: ID | null
}

export interface IRenderMarkdownOnQueryArguments {
    markdown: string
    options?: IMarkdownOptions | null
}

export interface IConfigurationSubjectOnQueryArguments {
    id: ID
}

export interface ISearchOnQueryArguments {
    /**
     * @default ""
     */
    query?: string | null
}

export interface ISurveyResponsesOnQueryArguments {
    first?: number | null
}

export interface IDocSitePageOnQueryArguments {
    path: string
}

export type Node =
    | IRepository
    | IGitCommit
    | IUser
    | IOrg
    | IOrganizationInvitation
    | IAccessToken
    | IExternalAccount
    | IGitRef
    | IRegistryExtension
    | IProductSubscription
    | IProductLicense

export interface INode {
    __typename: 'Node'
    id: ID
}

export interface IRepository {
    __typename: 'Repository'
    id: ID
    name: string

    /**
     * @deprecated "use name instead"
     */
    uri: string
    description: string
    language: string
    enabled: boolean
    createdAt: string
    updatedAt: string | null
    commit: IGitCommit | null
    mirrorInfo: IMirrorRepositoryInfo
    externalRepository: IExternalRepository | null

    /**
     * @deprecated "use Repository.mirrorInfo.cloneInProgress instead"
     */
    cloneInProgress: boolean
    lastIndexedRevOrLatest: IGitCommit | null
    textSearchIndex: IRepositoryTextSearchIndex | null
    url: string
    externalURLs: IExternalLink[]
    defaultBranch: IGitRef | null
    gitRefs: IGitRefConnection
    branches: IGitRefConnection
    tags: IGitRefConnection
    comparison: IRepositoryComparison
    contributors: IRepositoryContributorConnection
    redirectURL: string | null
    viewerCanAdminister: boolean
}

export interface ICommitOnRepositoryArguments {
    rev: string
    inputRevspec?: string | null
}

export interface IGitRefsOnRepositoryArguments {
    first?: number | null
    query?: string | null
    type?: GitRefType | null
    orderBy?: GitRefOrder | null
}

export interface IBranchesOnRepositoryArguments {
    first?: number | null
    query?: string | null
    orderBy?: GitRefOrder | null
}

export interface ITagsOnRepositoryArguments {
    first?: number | null
    query?: string | null
}

export interface IComparisonOnRepositoryArguments {
    base?: string | null
    head?: string | null
}

export interface IContributorsOnRepositoryArguments {
    revisionRange?: string | null
    after?: string | null
    path?: string | null
    first?: number | null
}

export interface IGitCommit {
    __typename: 'GitCommit'
    id: ID
    repository: IRepository
    oid: any
    abbreviatedOID: string
    author: ISignature
    committer: ISignature | null
    message: string
    subject: string
    body: string | null
    parents: IGitCommit[]
    url: string
    canonicalURL: string
    externalURLs: IExternalLink[]
    tree: IGitTree | null
    blob: IGitBlob | null
    file: File2 | null
    languages: string[]
    ancestors: IGitCommitConnection
    behindAhead: IBehindAheadCounts
    symbols: ISymbolConnection
}

export interface ITreeOnGitCommitArguments {
    /**
     * @default ""
     */
    path?: string | null

    /**
     * @default false
     */
    recursive?: boolean | null
}

export interface IBlobOnGitCommitArguments {
    path: string
}

export interface IFileOnGitCommitArguments {
    path: string
}

export interface IAncestorsOnGitCommitArguments {
    first?: number | null
    query?: string | null
    path?: string | null
}

export interface IBehindAheadOnGitCommitArguments {
    revspec: string
}

export interface ISymbolsOnGitCommitArguments {
    first?: number | null
    query?: string | null
}

export interface ISignature {
    __typename: 'Signature'
    person: IPerson
    date: string
}

export interface IPerson {
    __typename: 'Person'
    name: string
    email: string
    displayName: string
    avatarURL: string
    user: IUser | null
}

export interface IUser {
    __typename: 'User'
    id: ID
    username: string

    /**
     * @deprecated "use id instead"
     */
    sourcegraphID: number

    /**
     * @deprecated "use emails instead"
     */
    email: string
    displayName: string | null
    avatarURL: string | null
    url: string
    settingsURL: string
    createdAt: string
    updatedAt: string | null
    siteAdmin: boolean
    latestSettings: ISettings | null
    configurationCascade: IConfigurationCascade
    organizations: IOrgConnection
    organizationMemberships: IOrganizationMembershipConnection
    tags: string[]
    activity: IUserActivity
    emails: IUserEmail[]
    accessTokens: IAccessTokenConnection
    externalAccounts: IExternalAccountConnection
    session: ISession
    viewerCanAdminister: boolean
    surveyResponses: ISurveyResponse[]
    urlForSiteAdminBilling: string | null
}

export interface IAccessTokensOnUserArguments {
    first?: number | null
}

export interface IExternalAccountsOnUserArguments {
    first?: number | null
}

export type ConfigurationSubject = IUser | IOrg | ISite

export interface IConfigurationSubject {
    __typename: 'ConfigurationSubject'
    id: ID
    latestSettings: ISettings | null
    settingsURL: string
    viewerCanAdminister: boolean
    configurationCascade: IConfigurationCascade
}

export interface ISettings {
    __typename: 'Settings'
    id: number
    configuration: IConfiguration
    subject: ConfigurationSubject
    author: IUser | null
    createdAt: string

    /**
     * @deprecated "use configuration.contents instead"
     */
    contents: string
}

export interface IConfiguration {
    __typename: 'Configuration'
    contents: string
    messages: string[]
}

export interface IConfigurationCascade {
    __typename: 'ConfigurationCascade'
    subjects: ConfigurationSubject[]
    merged: IConfiguration
}

export interface IOrgConnection {
    __typename: 'OrgConnection'
    nodes: IOrg[]
    totalCount: number
}

export interface IOrg {
    __typename: 'Org'
    id: ID
    name: string
    displayName: string | null
    createdAt: string
    members: IUserConnection
    latestSettings: ISettings | null
    configurationCascade: IConfigurationCascade
    viewerPendingInvitation: IOrganizationInvitation | null
    viewerCanAdminister: boolean
    viewerIsMember: boolean
    url: string
    settingsURL: string
}

export interface IUserConnection {
    __typename: 'UserConnection'
    nodes: IUser[]
    totalCount: number
    pageInfo: IPageInfo
}

export interface IPageInfo {
    __typename: 'PageInfo'
    hasNextPage: boolean
}

export interface IOrganizationInvitation {
    __typename: 'OrganizationInvitation'
    id: ID
    organization: IOrg
    sender: IUser
    recipient: IUser
    createdAt: string
    notifiedAt: string | null
    respondedAt: string | null
    responseType: OrganizationInvitationResponseType | null
    respondURL: string | null
    revokedAt: string | null
}

export const enum OrganizationInvitationResponseType {
    ACCEPT = 'ACCEPT',
    REJECT = 'REJECT',
}

export interface IOrganizationMembershipConnection {
    __typename: 'OrganizationMembershipConnection'
    nodes: IOrganizationMembership[]
    totalCount: number
}

export interface IOrganizationMembership {
    __typename: 'OrganizationMembership'
    organization: IOrg
    user: IUser
    createdAt: string
    updatedAt: string
}

export interface IUserActivity {
    __typename: 'UserActivity'
    searchQueries: number
    pageViews: number
    codeIntelligenceActions: number
    lastActiveTime: string | null
    lastActiveCodeHostIntegrationTime: string | null
}

export interface IUserEmail {
    __typename: 'UserEmail'
    email: string
    verified: boolean
    verificationPending: boolean
    user: IUser
    viewerCanManuallyVerify: boolean
}

export interface IAccessTokenConnection {
    __typename: 'AccessTokenConnection'
    nodes: IAccessToken[]
    totalCount: number
    pageInfo: IPageInfo
}

export interface IAccessToken {
    __typename: 'AccessToken'
    id: ID
    subject: IUser
    scopes: string[]
    note: string
    creator: IUser
    createdAt: string
    lastUsedAt: string | null
}

export interface IExternalAccountConnection {
    __typename: 'ExternalAccountConnection'
    nodes: IExternalAccount[]
    totalCount: number
    pageInfo: IPageInfo
}

export interface IExternalAccount {
    __typename: 'ExternalAccount'
    id: ID
    user: IUser
    serviceType: string
    serviceID: string
    clientID: string
    accountID: string
    createdAt: string
    updatedAt: string
    refreshURL: string | null
    accountData: any | null
}

export interface ISession {
    __typename: 'Session'
    canSignOut: boolean
}

export interface ISurveyResponse {
    __typename: 'SurveyResponse'
    id: ID
    user: IUser | null
    email: string | null
    score: number
    reason: string | null
    better: string | null
    createdAt: string
}

export interface IExternalLink {
    __typename: 'ExternalLink'
    url: string
    serviceType: string | null
}

export interface IGitTree {
    __typename: 'GitTree'
    path: string
    isRoot: boolean
    name: string
    isDirectory: boolean
    commit: IGitCommit
    repository: IRepository
    url: string
    canonicalURL: string
    externalURLs: IExternalLink[]
    submodule: ISubmodule | null
    directories: IGitTree[]
    files: IFile[]
    entries: TreeEntry[]
    symbols: ISymbolConnection
    isSingleChild: boolean
}

export interface IDirectoriesOnGitTreeArguments {
    first?: number | null

    /**
     * @default false
     */
    recursive?: boolean | null
}

export interface IFilesOnGitTreeArguments {
    first?: number | null

    /**
     * @default false
     */
    recursive?: boolean | null
}

export interface IEntriesOnGitTreeArguments {
    first?: number | null

    /**
     * @default false
     */
    recursive?: boolean | null

    /**
     * @default false
     */
    recursiveSingleChild?: boolean | null
}

export interface ISymbolsOnGitTreeArguments {
    first?: number | null
    query?: string | null
}

export interface IIsSingleChildOnGitTreeArguments {
    first?: number | null

    /**
     * @default false
     */
    recursive?: boolean | null
}

export type TreeEntry = IGitTree | IGitBlob

export interface ITreeEntry {
    __typename: 'TreeEntry'
    path: string
    name: string
    isDirectory: boolean
    url: string
    canonicalURL: string
    externalURLs: IExternalLink[]
    symbols: ISymbolConnection
    submodule: ISubmodule | null
    isSingleChild: boolean
}

export interface ISymbolsOnTreeEntryArguments {
    first?: number | null
    query?: string | null
}

export interface IIsSingleChildOnTreeEntryArguments {
    first?: number | null

    /**
     * @default false
     */
    recursive?: boolean | null
}

export interface ISymbolConnection {
    __typename: 'SymbolConnection'
    nodes: ISymbol[]
    pageInfo: IPageInfo
}

export interface ISymbol {
    __typename: 'Symbol'
    name: string
    containerName: string | null
    kind: SymbolKind
    language: string
    location: ILocation
    url: string
    canonicalURL: string
}

export const enum SymbolKind {
    UNKNOWN = 'UNKNOWN',
    FILE = 'FILE',
    MODULE = 'MODULE',
    NAMESPACE = 'NAMESPACE',
    PACKAGE = 'PACKAGE',
    CLASS = 'CLASS',
    METHOD = 'METHOD',
    PROPERTY = 'PROPERTY',
    FIELD = 'FIELD',
    CONSTRUCTOR = 'CONSTRUCTOR',
    ENUM = 'ENUM',
    INTERFACE = 'INTERFACE',
    FUNCTION = 'FUNCTION',
    VARIABLE = 'VARIABLE',
    CONSTANT = 'CONSTANT',
    STRING = 'STRING',
    NUMBER = 'NUMBER',
    BOOLEAN = 'BOOLEAN',
    ARRAY = 'ARRAY',
    OBJECT = 'OBJECT',
    KEY = 'KEY',
    NULL = 'NULL',
    ENUMMEMBER = 'ENUMMEMBER',
    STRUCT = 'STRUCT',
    EVENT = 'EVENT',
    OPERATOR = 'OPERATOR',
    TYPEPARAMETER = 'TYPEPARAMETER',
}

export interface ILocation {
    __typename: 'Location'
    resource: IGitBlob
    range: IRange | null
    url: string
    canonicalURL: string
}

export interface IGitBlob {
    __typename: 'GitBlob'
    path: string
    name: string
    isDirectory: boolean
    content: string
    binary: boolean
    richHTML: string
    commit: IGitCommit
    repository: IRepository
    url: string
    canonicalURL: string
    externalURLs: IExternalLink[]
    blame: IHunk[]
    highlight: IHighlightedFile
    dependencyReferences: IDependencyReferences
    submodule: ISubmodule | null
    symbols: ISymbolConnection
    isSingleChild: boolean
}

export interface IBlameOnGitBlobArguments {
    startLine: number
    endLine: number
}

export interface IHighlightOnGitBlobArguments {
    disableTimeout: boolean
    isLightTheme: boolean
}

export interface IDependencyReferencesOnGitBlobArguments {
    Language: string
    Line: number
    Character: number
}

export interface ISymbolsOnGitBlobArguments {
    first?: number | null
    query?: string | null
}

export interface IIsSingleChildOnGitBlobArguments {
    first?: number | null

    /**
     * @default false
     */
    recursive?: boolean | null

    /**
     * @default false
     */
    recursiveSingleChild?: boolean | null
}

export type File2 = IGitBlob

export interface IFile2 {
    __typename: 'File2'
    path: string
    name: string
    isDirectory: boolean
    content: string
    binary: boolean
    richHTML: string
    url: string
    canonicalURL: string
    externalURLs: IExternalLink[]
    highlight: IHighlightedFile
    dependencyReferences: IDependencyReferences
    symbols: ISymbolConnection
}

export interface IHighlightOnFile2Arguments {
    disableTimeout: boolean
    isLightTheme: boolean
}

export interface IDependencyReferencesOnFile2Arguments {
    Language: string
    Line: number
    Character: number
}

export interface ISymbolsOnFile2Arguments {
    first?: number | null
    query?: string | null
}

export interface IHighlightedFile {
    __typename: 'HighlightedFile'
    aborted: boolean
    html: string
}

export interface IDependencyReferences {
    __typename: 'DependencyReferences'
    dependencyReferenceData: IDependencyReferencesData
    repoData: IRepoDataMap
}

export interface IDependencyReferencesData {
    __typename: 'DependencyReferencesData'
    references: IDependencyReference[]
    location: IDepLocation
}

export interface IDependencyReference {
    __typename: 'DependencyReference'
    dependencyData: string
    repoId: number
    hints: string
}

export interface IDepLocation {
    __typename: 'DepLocation'
    location: string
    symbol: string
}

export interface IRepoDataMap {
    __typename: 'RepoDataMap'
    repos: IRepository[]
    repoIds: number[]
}

export interface IHunk {
    __typename: 'Hunk'
    startLine: number
    endLine: number
    startByte: number
    endByte: number
    rev: string
    author: ISignature
    message: string
    commit: IGitCommit
}

export interface ISubmodule {
    __typename: 'Submodule'
    url: string
    commit: string
    path: string
}

export interface IRange {
    __typename: 'Range'
    start: IPosition
    end: IPosition
}

export interface IPosition {
    __typename: 'Position'
    line: number
    character: number
}

export interface IFile {
    __typename: 'File'
    path: string
    name: string
    isDirectory: boolean
    url: string
    repository: IRepository
}

export interface IGitCommitConnection {
    __typename: 'GitCommitConnection'
    nodes: IGitCommit[]
    pageInfo: IPageInfo
}

export interface IBehindAheadCounts {
    __typename: 'BehindAheadCounts'
    behind: number
    ahead: number
}

export interface IMirrorRepositoryInfo {
    __typename: 'MirrorRepositoryInfo'
    remoteURL: string
    cloneInProgress: boolean
    cloneProgress: string | null
    cloned: boolean
    updatedAt: string | null
}

export interface IExternalRepository {
    __typename: 'ExternalRepository'
    id: string
    serviceType: string
    serviceID: string
}

export interface IRepositoryTextSearchIndex {
    __typename: 'RepositoryTextSearchIndex'
    repository: IRepository
    status: IRepositoryTextSearchIndexStatus | null
    refs: IRepositoryTextSearchIndexedRef[]
}

export interface IRepositoryTextSearchIndexStatus {
    __typename: 'RepositoryTextSearchIndexStatus'
    updatedAt: string
    contentByteSize: number
    contentFilesCount: number
    indexByteSize: number
    indexShardsCount: number
}

export interface IRepositoryTextSearchIndexedRef {
    __typename: 'RepositoryTextSearchIndexedRef'
    ref: IGitRef
    indexed: boolean
    current: boolean
    indexedCommit: IGitObject | null
}

export interface IGitRef {
    __typename: 'GitRef'
    id: ID
    name: string
    abbrevName: string
    displayName: string
    prefix: string
    type: GitRefType
    target: IGitObject
    repository: IRepository
    url: string
}

export const enum GitRefType {
    GIT_BRANCH = 'GIT_BRANCH',
    GIT_TAG = 'GIT_TAG',
    GIT_REF_OTHER = 'GIT_REF_OTHER',
}

export interface IGitObject {
    __typename: 'GitObject'
    oid: any
    abbreviatedOID: string
    commit: IGitCommit | null
    type: GitObjectType
}

export const enum GitObjectType {
    GIT_COMMIT = 'GIT_COMMIT',
    GIT_TAG = 'GIT_TAG',
    GIT_TREE = 'GIT_TREE',
    GIT_BLOB = 'GIT_BLOB',
    GIT_UNKNOWN = 'GIT_UNKNOWN',
}

export const enum GitRefOrder {
    AUTHORED_OR_COMMITTED_AT = 'AUTHORED_OR_COMMITTED_AT',
}

export interface IGitRefConnection {
    __typename: 'GitRefConnection'
    nodes: IGitRef[]
    totalCount: number
    pageInfo: IPageInfo
}

export interface IRepositoryComparison {
    __typename: 'RepositoryComparison'
    range: IGitRevisionRange
    commits: IGitCommitConnection
    fileDiffs: IFileDiffConnection
}

export interface ICommitsOnRepositoryComparisonArguments {
    first?: number | null
}

export interface IFileDiffsOnRepositoryComparisonArguments {
    first?: number | null
}

export interface IGitRevisionRange {
    __typename: 'GitRevisionRange'
    expr: string
    base: GitRevSpec
    baseRevSpec: IGitRevSpecExpr
    head: GitRevSpec
    headRevSpec: IGitRevSpecExpr
    mergeBase: IGitObject | null
}

export type GitRevSpec = IGitRef | IGitRevSpecExpr | IGitObject

export interface IGitRevSpecExpr {
    __typename: 'GitRevSpecExpr'
    expr: string
    object: IGitObject | null
}

export interface IFileDiffConnection {
    __typename: 'FileDiffConnection'
    nodes: IFileDiff[]
    totalCount: number | null
    pageInfo: IPageInfo
    diffStat: IDiffStat
    rawDiff: string
}

export interface IFileDiff {
    __typename: 'FileDiff'
    oldPath: string | null
    oldFile: File2 | null
    newPath: string | null
    newFile: File2 | null
    mostRelevantFile: File2
    hunks: IFileDiffHunk[]
    stat: IDiffStat
    internalID: string
}

export interface IFileDiffHunk {
    __typename: 'FileDiffHunk'
    oldRange: IFileDiffHunkRange
    oldNoNewlineAt: boolean
    newRange: IFileDiffHunkRange
    section: string | null
    body: string
}

export interface IFileDiffHunkRange {
    __typename: 'FileDiffHunkRange'
    startLine: number
    lines: number
}

export interface IDiffStat {
    __typename: 'DiffStat'
    added: number
    changed: number
    deleted: number
}

export interface IRepositoryContributorConnection {
    __typename: 'RepositoryContributorConnection'
    nodes: IRepositoryContributor[]
    totalCount: number
    pageInfo: IPageInfo
}

export interface IRepositoryContributor {
    __typename: 'RepositoryContributor'
    person: IPerson
    count: number
    repository: IRepository
    commits: IGitCommitConnection
}

export interface ICommitsOnRepositoryContributorArguments {
    first?: number | null
}

export const enum RepoOrderBy {
    REPO_URI = 'REPO_URI',
    REPO_CREATED_AT = 'REPO_CREATED_AT',
}

export interface IRepositoryConnection {
    __typename: 'RepositoryConnection'
    nodes: IRepository[]
    totalCount: number | null
    pageInfo: IPageInfo
}

export interface ITotalCountOnRepositoryConnectionArguments {
    /**
     * @default false
     */
    precise?: boolean | null
}

export interface IPhabricatorRepo {
    __typename: 'PhabricatorRepo'
    name: string

    /**
     * @deprecated "use name instead"
     */
    uri: string
    callsign: string
    url: string
}

export const enum UserActivePeriod {
    TODAY = 'TODAY',
    THIS_WEEK = 'THIS_WEEK',
    THIS_MONTH = 'THIS_MONTH',
    ALL_TIME = 'ALL_TIME',
}

export interface IDiscussionThreadConnection {
    __typename: 'DiscussionThreadConnection'
    nodes: IDiscussionThread[]
    totalCount: number
    pageInfo: IPageInfo
}

export interface IDiscussionThread {
    __typename: 'DiscussionThread'
    id: ID
    author: IUser
    title: string
    target: DiscussionThreadTarget
    inlineURL: string | null
    createdAt: string
    updatedAt: string
    archivedAt: string | null
    comments: IDiscussionCommentConnection
}

export interface ICommentsOnDiscussionThreadArguments {
    first?: number | null
}

export type DiscussionThreadTarget = IDiscussionThreadTargetRepo

export interface IDiscussionThreadTargetRepo {
    __typename: 'DiscussionThreadTargetRepo'
    repository: IRepository
    path: string | null
    branch: IGitRef | null
    revision: IGitRef | null
    selection: IDiscussionThreadTargetRepoSelection | null
    relativePath: string | null
    relativeSelection: IDiscussionSelectionRange | null
}

export interface IRelativePathOnDiscussionThreadTargetRepoArguments {
    rev: string
}

export interface IRelativeSelectionOnDiscussionThreadTargetRepoArguments {
    rev: string
}

export interface IDiscussionThreadTargetRepoSelection {
    __typename: 'DiscussionThreadTargetRepoSelection'
    startLine: number
    startCharacter: number
    endLine: number
    endCharacter: number
    linesBefore: string[]
    lines: string[]
    linesAfter: string[]
}

export interface IDiscussionSelectionRange {
    __typename: 'DiscussionSelectionRange'
    startLine: number
    startCharacter: number
    endLine: number
    endCharacter: number
}

export interface IDiscussionCommentConnection {
    __typename: 'DiscussionCommentConnection'
    nodes: IDiscussionComment[]
    totalCount: number
    pageInfo: IPageInfo
}

export interface IDiscussionComment {
    __typename: 'DiscussionComment'
    id: ID
    thread: IDiscussionThread
    author: IUser
    contents: string
    html: string
    inlineURL: string | null
    createdAt: string
    updatedAt: string
    reports: string[]
    canReport: boolean
    canDelete: boolean
    canClearReports: boolean
}

export interface IHtmlOnDiscussionCommentArguments {
    options?: IMarkdownOptions | null
}

export interface IMarkdownOptions {
    alwaysNil?: string | null
}

export interface IClientConfigurationDetails {
    __typename: 'ClientConfigurationDetails'
    contentScriptUrls: string[]
    parentSourcegraph: IParentSourcegraphDetails
}

export interface IParentSourcegraphDetails {
    __typename: 'ParentSourcegraphDetails'
    url: string
}

export interface ISearch {
    __typename: 'Search'
    results: ISearchResults
    suggestions: SearchSuggestion[]
    stats: ISearchResultsStats
}

export interface ISuggestionsOnSearchArguments {
    first?: number | null
}

export interface ISearchResults {
    __typename: 'SearchResults'
    results: SearchResult[]
    resultCount: number
    approximateResultCount: string
    limitHit: boolean
    sparkline: number[]
    repositories: IRepository[]
    repositoriesSearched: IRepository[]
    indexedRepositoriesSearched: IRepository[]
    cloning: IRepository[]
    missing: IRepository[]
    timedout: IRepository[]
    indexUnavailable: boolean
    alert: ISearchAlert | null
    elapsedMilliseconds: number
    dynamicFilters: ISearchFilter[]
}

export type SearchResult = IFileMatch | ICommitSearchResult | IRepository

export interface IFileMatch {
    __typename: 'FileMatch'
    file: IGitBlob
    repository: IRepository

    /**
     * @deprecated "use the file field instead"
     */
    resource: string
    symbols: ISymbol[]
    lineMatches: ILineMatch[]
    limitHit: boolean
}

export interface ILineMatch {
    __typename: 'LineMatch'
    preview: string
    lineNumber: number
    offsetAndLengths: number[][]
    limitHit: boolean
}

export interface ICommitSearchResult {
    __typename: 'CommitSearchResult'
    commit: IGitCommit
    refs: IGitRef[]
    sourceRefs: IGitRef[]
    messagePreview: IHighlightedString | null
    diffPreview: IHighlightedString | null
}

export interface IHighlightedString {
    __typename: 'HighlightedString'
    value: string
    highlights: IHighlight[]
}

export interface IHighlight {
    __typename: 'Highlight'
    line: number
    character: number
    length: number
}

export interface ISearchAlert {
    __typename: 'SearchAlert'
    title: string
    description: string | null
    proposedQueries: ISearchQueryDescription[] | null
}

export interface ISearchQueryDescription {
    __typename: 'SearchQueryDescription'
    description: string | null
    query: string
}

export interface ISearchFilter {
    __typename: 'SearchFilter'
    value: string
    label: string
    count: number
    limitHit: boolean
    kind: string
}

export type SearchSuggestion = IRepository | IFile | ISymbol

export interface ISearchResultsStats {
    __typename: 'SearchResultsStats'
    approximateResultCount: string
    sparkline: number[]
}

export interface ISavedQuery {
    __typename: 'SavedQuery'
    id: ID
    subject: ConfigurationSubject
    key: string | null
    index: number
    description: string
    query: string
    showOnHomepage: boolean
    notify: boolean
    notifySlack: boolean
}

export interface IRepoGroup {
    __typename: 'RepoGroup'
    name: string
    repositories: string[]
}

export interface ISite {
    __typename: 'Site'
    id: ID
    siteID: string
    configuration: ISiteConfiguration
    latestSettings: ISettings | null
    configurationCascade: IConfigurationCascade
    settingsURL: string
    canReloadSite: boolean
    viewerCanAdminister: boolean
    accessTokens: IAccessTokenConnection
    authProviders: IAuthProviderConnection
    externalAccounts: IExternalAccountConnection
    buildVersion: string
    productVersion: string
    updateCheck: IUpdateCheck
    needsRepositoryConfiguration: boolean
    noRepositoriesEnabled: boolean
    alerts: IAlert[]
    hasCodeIntelligence: boolean
    disableBuiltInSearches: boolean
    sendsEmailVerificationEmails: boolean
    productSubscription: IProductSubscriptionStatus
    activity: ISiteUsageStatistics
}

export interface IAccessTokensOnSiteArguments {
    first?: number | null
}

export interface IExternalAccountsOnSiteArguments {
    first?: number | null
    user?: ID | null
    serviceType?: string | null
    serviceID?: string | null
    clientID?: string | null
}

export interface IActivityOnSiteArguments {
    days?: number | null
    weeks?: number | null
    months?: number | null
}

export interface ISiteConfiguration {
    __typename: 'SiteConfiguration'
    effectiveContents: string
    validationMessages: string[]
    canUpdate: boolean
    source: string
}

export interface IAuthProviderConnection {
    __typename: 'AuthProviderConnection'
    nodes: IAuthProvider[]
    totalCount: number
    pageInfo: IPageInfo
}

export interface IAuthProvider {
    __typename: 'AuthProvider'
    serviceType: string
    serviceID: string
    clientID: string
    displayName: string
    isBuiltin: boolean
    authenticationURL: string | null
}

export interface IUpdateCheck {
    __typename: 'UpdateCheck'
    pending: boolean
    checkedAt: string | null
    errorMessage: string | null
    updateVersionAvailable: string | null
}

export interface IAlert {
    __typename: 'Alert'
    type: AlertType
    message: string
    isDismissibleWithKey: string | null
}

export const enum AlertType {
    INFO = 'INFO',
    WARNING = 'WARNING',
    ERROR = 'ERROR',
}

export interface IProductSubscriptionStatus {
    __typename: 'ProductSubscriptionStatus'
    productNameWithBrand: string
    actualUserCount: number
    license: IProductLicenseInfo | null
}

export interface IProductLicenseInfo {
    __typename: 'ProductLicenseInfo'
    productNameWithBrand: string
    tags: string[]
    userCount: number
    expiresAt: string
}

export interface ISiteUsageStatistics {
    __typename: 'SiteActivity'
    daus: ISiteUsageStatisticsPeriod[]
    waus: ISiteUsageStatisticsPeriod[]
    maus: ISiteUsageStatisticsPeriod[]
}

export interface ISiteUsageStatisticsPeriod {
    __typename: 'SiteActivityPeriod'
    startTime: string
    userCount: number
    registeredUserCount: number
    anonymousUserCount: number
    integrationUserCount: number
}

export interface ISurveyResponseConnection {
    __typename: 'SurveyResponseConnection'
    nodes: ISurveyResponse[]
    totalCount: number
    last30DaysCount: number
    averageScore: number
    netPromoterScore: number
}

export interface IExtensionRegistry {
    __typename: 'ExtensionRegistry'
    extension: IRegistryExtension | null
    extensions: IRegistryExtensionConnection
    publishers: IRegistryPublisherConnection
    viewerPublishers: RegistryPublisher[]
    localExtensionIDPrefix: string | null
}

export interface IExtensionOnExtensionRegistryArguments {
    extensionID: string
}

export interface IExtensionsOnExtensionRegistryArguments {
    first?: number | null
    publisher?: ID | null
    query?: string | null

    /**
     * @default true
     */
    local?: boolean | null

    /**
     * @default true
     */
    remote?: boolean | null
    prioritizeExtensionIDs?: string[] | null
}

export interface IPublishersOnExtensionRegistryArguments {
    first?: number | null
}

export interface IRegistryExtension {
    __typename: 'RegistryExtension'
    id: ID
    uuid: string
    publisher: RegistryPublisher | null
    extensionID: string
    extensionIDWithoutRegistry: string
    name: string
    manifest: IExtensionManifest | null
    createdAt: string | null
    updatedAt: string | null
    url: string
    remoteURL: string | null
    registryName: string
    isLocal: boolean
    viewerCanAdminister: boolean
}

export type RegistryPublisher = IUser | IOrg

export interface IExtensionManifest {
    __typename: 'ExtensionManifest'
    raw: string
    title: string | null
    description: string | null
    bundleURL: string | null
}

export interface IRegistryExtensionConnection {
    __typename: 'RegistryExtensionConnection'
    nodes: IRegistryExtension[]
    totalCount: number
    pageInfo: IPageInfo
    url: string | null
    error: string | null
}

export interface IRegistryPublisherConnection {
    __typename: 'RegistryPublisherConnection'
    nodes: RegistryPublisher[]
    totalCount: number
    pageInfo: IPageInfo
}

export interface IDocSitePage {
    __typename: 'DocSitePage'
    title: string
    contentHTML: string
    indexHTML: string
    filePath: string
}

export interface IDotcomQuery {
    __typename: 'DotcomQuery'
    productSubscription: IProductSubscription
    productSubscriptions: IProductSubscriptionConnection
    previewProductSubscriptionInvoice: IProductSubscriptionPreviewInvoice
    productLicenses: IProductLicenseConnection
    productPlans: IProductPlan[]
}

export interface IProductSubscriptionOnDotcomQueryArguments {
    uuid: string
}

export interface IProductSubscriptionsOnDotcomQueryArguments {
    first?: number | null
    account?: ID | null
}

export interface IPreviewProductSubscriptionInvoiceOnDotcomQueryArguments {
    account?: ID | null
    subscriptionToUpdate?: ID | null
    productSubscription: IProductSubscriptionInput
}

export interface IProductLicensesOnDotcomQueryArguments {
    first?: number | null
    licenseKeySubstring?: string | null
    productSubscriptionID?: ID | null
}

export interface IProductSubscription {
    __typename: 'ProductSubscription'
    id: ID
    uuid: string
    name: string
    account: IUser | null
    invoiceItem: IProductSubscriptionInvoiceItem | null
    events: IProductSubscriptionEvent[]
    activeLicense: IProductLicense | null
    productLicenses: IProductLicenseConnection
    createdAt: string
    isArchived: boolean
    url: string
    urlForSiteAdmin: string | null
    urlForSiteAdminBilling: string | null
}

export interface IProductLicensesOnProductSubscriptionArguments {
    first?: number | null
}

export interface IProductSubscriptionInvoiceItem {
    __typename: 'ProductSubscriptionInvoiceItem'
    plan: IProductPlan
    userCount: number
    expiresAt: string
}

export interface IProductPlan {
    __typename: 'ProductPlan'
    billingPlanID: string
    productPlanID: string
    name: string
    nameWithBrand: string
    pricePerUserPerYear: number
    minQuantity: number | null
    tiersMode: string
    planTiers: IPlanTier[]
}

export interface IPlanTier {
    __typename: 'PlanTier'
    unitAmount: number
    upTo: number
}

export interface IProductSubscriptionEvent {
    __typename: 'ProductSubscriptionEvent'
    id: string
    date: string
    title: string
    description: string | null
    url: string | null
}

export interface IProductLicense {
    __typename: 'ProductLicense'
    id: ID
    subscription: IProductSubscription
    info: IProductLicenseInfo | null
    licenseKey: string
    createdAt: string
}

export interface IProductLicenseConnection {
    __typename: 'ProductLicenseConnection'
    nodes: IProductLicense[]
    totalCount: number
    pageInfo: IPageInfo
}

export interface IProductSubscriptionConnection {
    __typename: 'ProductSubscriptionConnection'
    nodes: IProductSubscription[]
    totalCount: number
    pageInfo: IPageInfo
}

export interface IProductSubscriptionInput {
    billingPlanID: string
    userCount: number
}

export interface IProductSubscriptionPreviewInvoice {
    __typename: 'ProductSubscriptionPreviewInvoice'
    price: number
    prorationDate: string | null
    isDowngradeRequiringManualIntervention: boolean
    beforeInvoiceItem: IProductSubscriptionInvoiceItem | null
    afterInvoiceItem: IProductSubscriptionInvoiceItem
}

export interface IMutation {
    __typename: 'Mutation'
    updateUser: IEmptyResponse
    createOrganization: IOrg
    updateOrganization: IOrg
    deleteOrganization: IEmptyResponse | null
    addRepository: IRepository
    setRepositoryEnabled: IEmptyResponse | null
    setAllRepositoriesEnabled: IEmptyResponse | null
    checkMirrorRepositoryConnection: ICheckMirrorRepositoryConnectionResult
    updateMirrorRepository: IEmptyResponse
    updateAllMirrorRepositories: IEmptyResponse
    deleteRepository: IEmptyResponse | null
    createUser: ICreateUserResult
    randomizeUserPassword: IRandomizeUserPasswordResult
    addUserEmail: IEmptyResponse
    removeUserEmail: IEmptyResponse
    setUserEmailVerified: IEmptyResponse
    deleteUser: IEmptyResponse | null
    updatePassword: IEmptyResponse | null
    createAccessToken: ICreateAccessTokenResult
    deleteAccessToken: IEmptyResponse
    deleteExternalAccount: IEmptyResponse
    inviteUserToOrganization: IInviteUserToOrganizationResult
    respondToOrganizationInvitation: IEmptyResponse
    resendOrganizationInvitationNotification: IEmptyResponse
    revokeOrganizationInvitation: IEmptyResponse
    addUserToOrganization: IEmptyResponse
    removeUserFromOrganization: IEmptyResponse | null
    setTag: IEmptyResponse
    addPhabricatorRepo: IEmptyResponse | null
    resolvePhabricatorDiff: IGitCommit | null
    logUserEvent: IEmptyResponse | null
    sendSavedSearchTestNotification: IEmptyResponse | null
    configurationMutation: IConfigurationMutation | null
    updateSiteConfiguration: boolean
    discussions: IDiscussionsMutation | null
    setUserIsSiteAdmin: IEmptyResponse | null
    reloadSite: IEmptyResponse | null
    submitSurvey: IEmptyResponse | null
    extensionRegistry: IExtensionRegistryMutation
    dotcom: IDotcomMutation
}

export interface IUpdateUserOnMutationArguments {
    user: ID
    username?: string | null
    displayName?: string | null
    avatarURL?: string | null
}

export interface ICreateOrganizationOnMutationArguments {
    name: string
    displayName?: string | null
}

export interface IUpdateOrganizationOnMutationArguments {
    id: ID
    displayName?: string | null
}

export interface IDeleteOrganizationOnMutationArguments {
    organization: ID
}

export interface IAddRepositoryOnMutationArguments {
    name: string
}

export interface ISetRepositoryEnabledOnMutationArguments {
    repository: ID
    enabled: boolean
}

export interface ISetAllRepositoriesEnabledOnMutationArguments {
    enabled: boolean
}

export interface ICheckMirrorRepositoryConnectionOnMutationArguments {
    repository?: ID | null
    name?: string | null
}

export interface IUpdateMirrorRepositoryOnMutationArguments {
    repository: ID
}

export interface IDeleteRepositoryOnMutationArguments {
    repository: ID
}

export interface ICreateUserOnMutationArguments {
    username: string
    email?: string | null
}

export interface IRandomizeUserPasswordOnMutationArguments {
    user: ID
}

export interface IAddUserEmailOnMutationArguments {
    user: ID
    email: string
}

export interface IRemoveUserEmailOnMutationArguments {
    user: ID
    email: string
}

export interface ISetUserEmailVerifiedOnMutationArguments {
    user: ID
    email: string
    verified: boolean
}

export interface IDeleteUserOnMutationArguments {
    user: ID
    hard?: boolean | null
}

export interface IUpdatePasswordOnMutationArguments {
    oldPassword: string
    newPassword: string
}

export interface ICreateAccessTokenOnMutationArguments {
    user: ID
    scopes: string[]
    note: string
}

export interface IDeleteAccessTokenOnMutationArguments {
    byID?: ID | null
    byToken?: string | null
}

export interface IDeleteExternalAccountOnMutationArguments {
    externalAccount: ID
}

export interface IInviteUserToOrganizationOnMutationArguments {
    organization: ID
    username: string
}

export interface IRespondToOrganizationInvitationOnMutationArguments {
    organizationInvitation: ID
    responseType: OrganizationInvitationResponseType
}

export interface IResendOrganizationInvitationNotificationOnMutationArguments {
    organizationInvitation: ID
}

export interface IRevokeOrganizationInvitationOnMutationArguments {
    organizationInvitation: ID
}

export interface IAddUserToOrganizationOnMutationArguments {
    organization: ID
    username: string
}

export interface IRemoveUserFromOrganizationOnMutationArguments {
    user: ID
    organization: ID
}

export interface ISetTagOnMutationArguments {
    node: ID
    tag: string
    present: boolean
}

export interface IAddPhabricatorRepoOnMutationArguments {
    callsign: string
    name?: string | null
    uri?: string | null
    url: string
}

export interface IResolvePhabricatorDiffOnMutationArguments {
    repoName: string
    diffID: ID
    baseRev: string
    patch?: string | null
    description?: string | null
    authorName?: string | null
    authorEmail?: string | null
    date?: string | null
}

export interface ILogUserEventOnMutationArguments {
    event: UserEvent
    userCookieID: string
}

export interface ISendSavedSearchTestNotificationOnMutationArguments {
    id: ID
}

export interface IConfigurationMutationOnMutationArguments {
    input: IConfigurationMutationGroupInput
}

export interface IUpdateSiteConfigurationOnMutationArguments {
    input: string
}

export interface ISetUserIsSiteAdminOnMutationArguments {
    userID: ID
    siteAdmin: boolean
}

export interface ISubmitSurveyOnMutationArguments {
    input: ISurveySubmissionInput
}

export interface IEmptyResponse {
    __typename: 'EmptyResponse'
    alwaysNil: string | null
}

export interface ICheckMirrorRepositoryConnectionResult {
    __typename: 'CheckMirrorRepositoryConnectionResult'
    error: string | null
}

export interface ICreateUserResult {
    __typename: 'CreateUserResult'
    user: IUser
    resetPasswordURL: string | null
}

export interface IRandomizeUserPasswordResult {
    __typename: 'RandomizeUserPasswordResult'
    resetPasswordURL: string | null
}

export interface ICreateAccessTokenResult {
    __typename: 'CreateAccessTokenResult'
    id: ID
    token: string
}

export interface IInviteUserToOrganizationResult {
    __typename: 'InviteUserToOrganizationResult'
    sentInvitationEmail: boolean
    invitationURL: string
}

export const enum UserEvent {
    PAGEVIEW = 'PAGEVIEW',
    SEARCHQUERY = 'SEARCHQUERY',
    CODEINTEL = 'CODEINTEL',
    CODEINTELINTEGRATION = 'CODEINTELINTEGRATION',
}

export interface IConfigurationMutationGroupInput {
    subject: ID
    lastID?: number | null
}

export interface IConfigurationMutation {
    __typename: 'ConfigurationMutation'
    editConfiguration: IUpdateConfigurationPayload | null
    overwriteConfiguration: IUpdateConfigurationPayload | null
    createSavedQuery: ISavedQuery
    updateSavedQuery: ISavedQuery
    deleteSavedQuery: IEmptyResponse | null
}

export interface IEditConfigurationOnConfigurationMutationArguments {
    edit: IConfigurationEdit
}

export interface IOverwriteConfigurationOnConfigurationMutationArguments {
    contents?: string | null
}

export interface ICreateSavedQueryOnConfigurationMutationArguments {
    description: string
    query: string

    /**
     * @default false
     */
    showOnHomepage?: boolean | null

    /**
     * @default false
     */
    notify?: boolean | null

    /**
     * @default false
     */
    notifySlack?: boolean | null

    /**
     * @default false
     */
    disableSubscriptionNotifications?: boolean | null
}

export interface IUpdateSavedQueryOnConfigurationMutationArguments {
    id: ID
    description?: string | null
    query?: string | null

    /**
     * @default false
     */
    showOnHomepage?: boolean | null

    /**
     * @default false
     */
    notify?: boolean | null

    /**
     * @default false
     */
    notifySlack?: boolean | null
}

export interface IDeleteSavedQueryOnConfigurationMutationArguments {
    id: ID

    /**
     * @default false
     */
    disableSubscriptionNotifications?: boolean | null
}

export interface IConfigurationEdit {
    keyPath: IKeyPathSegment[]
    value?: any | null

    /**
     * @default false
     */
    valueIsJSONCEncodedString?: boolean | null
}

export interface IKeyPathSegment {
    property?: string | null
    index?: number | null
}

export interface IUpdateConfigurationPayload {
    __typename: 'UpdateConfigurationPayload'
    empty: IEmptyResponse | null
}

export interface IDiscussionsMutation {
    __typename: 'DiscussionsMutation'
    createThread: IDiscussionThread
    updateThread: IDiscussionThread | null
    addCommentToThread: IDiscussionThread
    updateComment: IDiscussionThread
}

export interface ICreateThreadOnDiscussionsMutationArguments {
    input: IDiscussionThreadCreateInput
}

export interface IUpdateThreadOnDiscussionsMutationArguments {
    input: IDiscussionThreadUpdateInput
}

export interface IAddCommentToThreadOnDiscussionsMutationArguments {
    threadID: ID
    contents: string
}

export interface IUpdateCommentOnDiscussionsMutationArguments {
    input: IDiscussionCommentUpdateInput
}

export interface IDiscussionThreadCreateInput {
    title?: string | null
    contents: string
    targetRepo?: IDiscussionThreadTargetRepoInput | null
}

export interface IDiscussionThreadTargetRepoInput {
    repositoryID?: ID | null
    repositoryName?: string | null
    repositoryGitCloneURL?: string | null
    path?: string | null
    branch?: string | null
    revision?: any | null
    selection?: IDiscussionThreadTargetRepoSelectionInput | null
}

export interface IDiscussionThreadTargetRepoSelectionInput {
    startLine: number
    startCharacter: number
    endLine: number
    endCharacter: number
    linesBefore?: string[] | null
    lines?: string[] | null
    linesAfter?: string[] | null
}

export interface IDiscussionThreadUpdateInput {
    ThreadID: ID
    Archive?: boolean | null
    Delete?: boolean | null
}

export interface IDiscussionCommentUpdateInput {
    commentID: ID
    delete?: boolean | null
    report?: string | null
    clearReports?: boolean | null
}

export interface ISurveySubmissionInput {
    email?: string | null
    score: number
    reason?: string | null
    better?: string | null
}

export interface IExtensionRegistryMutation {
    __typename: 'ExtensionRegistryMutation'
    createExtension: IExtensionRegistryCreateExtensionResult
    updateExtension: IExtensionRegistryUpdateExtensionResult
    deleteExtension: IEmptyResponse
    publishExtension: IExtensionRegistryCreateExtensionResult
}

export interface ICreateExtensionOnExtensionRegistryMutationArguments {
    publisher: ID
    name: string
}

export interface IUpdateExtensionOnExtensionRegistryMutationArguments {
    extension: ID
    name?: string | null
}

export interface IDeleteExtensionOnExtensionRegistryMutationArguments {
    extension: ID
}

export interface IPublishExtensionOnExtensionRegistryMutationArguments {
    extensionID: string
    manifest: string
    bundle?: string | null
    sourceMap?: string | null

    /**
     * @default false
     */
    force?: boolean | null
}

export interface IExtensionRegistryCreateExtensionResult {
    __typename: 'ExtensionRegistryCreateExtensionResult'
    extension: IRegistryExtension
}

export interface IExtensionRegistryUpdateExtensionResult {
    __typename: 'ExtensionRegistryUpdateExtensionResult'
    extension: IRegistryExtension
}

export interface IDotcomMutation {
    __typename: 'DotcomMutation'
    setUserBilling: IEmptyResponse
    createProductSubscription: IProductSubscription
    setProductSubscriptionBilling: IEmptyResponse
    generateProductLicenseForSubscription: IProductLicense
    createPaidProductSubscription: ICreatePaidProductSubscriptionResult
    updatePaidProductSubscription: IUpdatePaidProductSubscriptionResult
    archiveProductSubscription: IEmptyResponse
}

export interface ISetUserBillingOnDotcomMutationArguments {
    user: ID
    billingCustomerID?: string | null
}

export interface ICreateProductSubscriptionOnDotcomMutationArguments {
    accountID: ID
}

export interface ISetProductSubscriptionBillingOnDotcomMutationArguments {
    id: ID
    billingSubscriptionID?: string | null
}

export interface IGenerateProductLicenseForSubscriptionOnDotcomMutationArguments {
    productSubscriptionID: ID
    license: IProductLicenseInput
}

export interface ICreatePaidProductSubscriptionOnDotcomMutationArguments {
    accountID: ID
    productSubscription: IProductSubscriptionInput
    paymentToken: string
}

export interface IUpdatePaidProductSubscriptionOnDotcomMutationArguments {
    subscriptionID: ID
    update: IProductSubscriptionInput
    paymentToken: string
}

export interface IArchiveProductSubscriptionOnDotcomMutationArguments {
    id: ID
}

export interface IProductLicenseInput {
    tags: string[]
    userCount: number
    expiresAt: number
}

export interface ICreatePaidProductSubscriptionResult {
    __typename: 'CreatePaidProductSubscriptionResult'
    productSubscription: IProductSubscription
}

export interface IUpdatePaidProductSubscriptionResult {
    __typename: 'UpdatePaidProductSubscriptionResult'
    productSubscription: IProductSubscription
}

export interface IDiff {
    __typename: 'Diff'
    repository: IRepository
    range: IGitRevisionRange
}

export interface IDiffSearchResult {
    __typename: 'DiffSearchResult'
    diff: IDiff
    preview: IHighlightedString
}

export interface IRefFields {
    __typename: 'RefFields'
    refLocation: IRefLocation | null
    uri: IURI | null
}

export interface IRefLocation {
    __typename: 'RefLocation'
    startLineNumber: number
    startColumn: number
    endLineNumber: number
    endColumn: number
}

export interface IURI {
    __typename: 'URI'
    host: string
    fragment: string
    path: string
    query: string
    scheme: string
}

export interface IDeploymentConfiguration {
    __typename: 'DeploymentConfiguration'
    email: string | null
    siteID: string | null
}

export interface IExtensionRegistryPublishExtensionResult {
    __typename: 'ExtensionRegistryPublishExtensionResult'
    extension: IRegistryExtension
}
