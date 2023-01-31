import { useCallback, useEffect, useRef, useState } from 'react'

import { mdiFileDocumentOutline, mdiSourceRepository, mdiFolderOutline, mdiFolderOpenOutline } from '@mdi/js'
import classNames from 'classnames'
import { useNavigate } from 'react-router-dom-v5-compat'

import { gql, useQuery } from '@sourcegraph/http-client'
import { TelemetryService } from '@sourcegraph/shared/src/telemetry/telemetryService'
import {
    Alert,
    ErrorMessage,
    Icon,
    TreeNode as WildcardTreeNode,
    Link,
    LoadingSpinner,
    Tree,
    Tooltip,
    ErrorAlert,
} from '@sourcegraph/wildcard'

import { FileTreeEntriesResult, FileTreeEntriesVariables } from '../graphql-operations'
import { MAX_TREE_ENTRIES } from '../tree/constants'
import { dirname } from '../util/path'

import styles from './RepoRevisionSidebarFileTree.module.scss'

const QUERY = gql`
    query FileTreeEntries(
        $repoName: String!
        $revision: String!
        $commitID: String!
        $filePath: String!
        $ancestors: Boolean!
        $first: Int
    ) {
        repository(name: $repoName) {
            commit(rev: $commitID, inputRevspec: $revision) {
                tree(path: $filePath) {
                    isRoot
                    url
                    entries(first: $first, ancestors: $ancestors) {
                        name
                        path
                        isDirectory
                        url
                        submodule {
                            url
                            commit
                        }
                        isSingleChild
                    }
                }
            }
        }
    }
`

// TODO(philipp-spiess): Figure out why we can't use a GraphQL fragment here.
// The issue is that on `TreeEntry` does not match because the type is either
// `GitTree` or `GitBlob` and it needs to match?
type FileTreeEntry = Extract<
    Extract<
        Extract<FileTreeEntriesResult['repository'], { __typename?: 'Repository' }>['commit'],
        { __typename?: 'GitCommit' }
    >['tree'],
    { __typename?: 'GitTree' }
>['entries'][number]

interface Props {
    // Instead of showing a `..` indicator and only loading the current dirs
    // entries, it will use the Ancestors query to load all entries of all
    // parent directories.
    alwaysLoadAncestors: boolean
    commitID: string
    initialFilePath: string
    initialFilePathIsDirectory: boolean
    filePath: string
    filePathIsDirectory: boolean
    onExpandParent: (parent: string) => void
    repoName: string
    revision: string
    telemetryService: TelemetryService
}
export const RepoRevisionSidebarFileTree: React.FunctionComponent<Props> = props => {
    const { telemetryService, onExpandParent, alwaysLoadAncestors } = props

    // Ensure that the initial file path does not update when the props change
    const [initialFilePath] = useState(() => {
        let path = props.initialFilePathIsDirectory ? props.initialFilePath : dirname(props.initialFilePath)
        if (path === '.') {
            path = ''
        }
        return path
    })
    const [treeData, setTreeData] = useState<TreeData | null>(null)

    const defaultNodeId = treeData?.pathToId.get(props.initialFilePath)
    const defaultNode = defaultNodeId ? treeData?.nodes[defaultNodeId] : undefined
    const allParentsOfDefaultNode = treeData
        ? getAllParentsOfPath(treeData, defaultNode?.path ?? '').map(node => node.id)
        : []
    const defaultSelectedIds = defaultNodeId ? [defaultNodeId] : []
    const defaultExpandedIds =
        treeData && defaultNode && defaultNodeId
            ? defaultNode?.isBranch
                ? [defaultNodeId, ...allParentsOfDefaultNode]
                : allParentsOfDefaultNode
            : []

    const [selectedIds, setSelectedIds] = useState<number[]>(defaultSelectedIds)

    const navigate = useNavigate()

    const [defaultVariables] = useState({
        repoName: props.repoName,
        revision: props.revision,
        commitID: props.commitID,
        first: MAX_TREE_ENTRIES,
    })

    const { error, loading, refetch } = useQuery<FileTreeEntriesResult, FileTreeEntriesVariables>(QUERY, {
        variables: {
            ...defaultVariables,
            filePath: initialFilePath,
            ancestors: alwaysLoadAncestors,
        },
        onCompleted(data) {
            const rootTreeUrl = data?.repository?.commit?.tree?.url
            const entries = data?.repository?.commit?.tree?.entries
            if (!entries || !rootTreeUrl) {
                throw new Error('No entries or root data')
            }
            if (treeData === null) {
                setTreeData(
                    appendTreeData(
                        createTreeData(alwaysLoadAncestors ? '' : initialFilePath),
                        entries,
                        rootTreeUrl,
                        alwaysLoadAncestors
                    )
                )
            } else {
                setTreeData(treeData => appendTreeData(treeData!, entries, rootTreeUrl, alwaysLoadAncestors))
            }
        },
    })

    const onLoadData = useCallback(
        async ({ element }: { element: TreeNode }) => {
            const fullPath = element.path
            const alreadyLoaded = element.children?.length > 0 || treeData?.loadedIds.has(element.id)
            if (alreadyLoaded || !element.isBranch) {
                return
            }

            telemetryService.log('FileTreeLoadDirectory')
            await refetch({
                ...defaultVariables,
                filePath: fullPath,
                ancestors: false,
            })

            setTreeData(treeData => setLoadedPath(treeData!, fullPath))
        },
        [defaultVariables, refetch, treeData?.loadedIds, telemetryService]
    )

    const defaultSelectFiredRef = useRef<boolean>(false)
    const onSelect = useCallback(
        ({ element, isSelected }: { element: TreeNode; isSelected: boolean }) => {
            if (!isSelected) {
                return
            }

            // On the initial rendering, an onSelect event is fired for the
            // default node. We don't want to navigate to that node though.
            if (defaultSelectFiredRef.current === false && element.id === defaultNodeId) {
                defaultSelectFiredRef.current = true
                return
            }

            if (element.dotdot) {
                telemetryService.log('FileTreeLoadParent')
                navigate(element.dotdot)

                let parent = props.initialFilePathIsDirectory
                    ? dirname(initialFilePath)
                    : dirname(dirname(initialFilePath))
                if (parent === '.') {
                    parent = ''
                }
                onExpandParent(parent)
                return
            }

            if (element.entry) {
                telemetryService.log('FileTreeClick')
                navigate(element.entry.url)
            }
            setSelectedIds([element.id])
        },
        [defaultNodeId, telemetryService, navigate, props.initialFilePathIsDirectory, initialFilePath, onExpandParent]
    )

    // We need a mutable reference to the tree data since we don't want the
    // below hook to run when the tree data changes.
    const treeDataRef = useRef<TreeData | null>(treeData)
    useEffect(() => {
        treeDataRef.current = treeData
    }, [treeData])
    useEffect(() => {
        const id = treeDataRef.current?.pathToId.get(props.filePath)
        if (id) {
            setSelectedIds([id])
        } else {
            // When a file is opened that is not inside the tree, we want the tree
            // to expand to the parent directory of the file.
            let path = props.filePathIsDirectory ? props.filePath : dirname(props.filePath)
            if (path === '.') {
                path = ''
            }
            if (props.initialFilePath.length > path.length && props.initialFilePath.startsWith(path)) {
                onExpandParent(path)
            }
        }
    }, [onExpandParent, props.filePath, props.filePathIsDirectory, props.initialFilePath])

    if (error) {
        return (
            <Alert variant="danger">
                <ErrorMessage error={error.message} />
            </Alert>
        )
    }
    if (loading || treeData === null) {
        return <LoadingSpinner />
    }

    return (
        <Tree<TreeNode>
            data={treeData.nodes}
            aria-label="file tree"
            selectedIds={selectedIds}
            defaultExpandedIds={defaultExpandedIds}
            onSelect={onSelect}
            onLoadData={onLoadData}
            renderNode={renderNode}
        />
    )
}

function renderNode({
    element,
    isBranch,
    isExpanded,
    handleSelect,
    props,
}: {
    element: TreeNode
    isBranch: boolean
    isExpanded: boolean
    handleSelect: (event: React.MouseEvent) => {}
    props: { className: string }
}): React.ReactNode {
    const { entry, error, dotdot } = element
    const submodule = entry?.submodule
    const name = entry?.name
    const url = entry?.url

    if (error) {
        return <ErrorAlert {...props} className={classNames(props.className, 'm-0')} variant="note" error={error} />
    }

    if (dotdot) {
        return (
            <Link
                {...props}
                to={dotdot}
                onClick={event => {
                    event.preventDefault()
                    handleSelect(event)
                }}
            >
                <Icon
                    svgPath={mdiFolderOutline}
                    className={classNames('mr-1', styles.icon)}
                    aria-label="Load parent directory"
                />
                ..
            </Link>
        )
    }

    if (submodule) {
        const rev = submodule.commit.slice(0, 7)
        const title = `${name} @ ${rev}`
        const tooltip = `Submodule: ${submodule.url}`

        if (url) {
            return (
                <Tooltip content={tooltip}>
                    <Link
                        {...props}
                        to={url}
                        onClick={event => {
                            event.preventDefault()
                            handleSelect(event)
                        }}
                    >
                        <Icon
                            svgPath={mdiSourceRepository}
                            className={classNames('mr-1', styles.icon)}
                            aria-label={tooltip}
                        />
                        {title}
                    </Link>
                </Tooltip>
            )
        }
        return (
            <Tooltip content={tooltip}>
                <span {...props}>
                    <Icon
                        svgPath={mdiSourceRepository}
                        className={classNames('mr-1', styles.icon)}
                        aria-label={tooltip}
                    />
                    {title}
                </span>
            </Tooltip>
        )
    }

    return (
        <Link
            {...props}
            to={url ?? '#'}
            onClick={event => {
                event.preventDefault()
                handleSelect(event)
            }}
        >
            <Icon
                svgPath={isBranch ? (isExpanded ? mdiFolderOpenOutline : mdiFolderOutline) : mdiFileDocumentOutline}
                className={classNames('mr-1', styles.icon)}
                aria-hidden={true}
            />
            {name}
        </Link>
    )
}

type TreeNode = WildcardTreeNode & {
    path: string
    entry: FileTreeEntry | null
    error: string | null
    dotdot: string | null
}
interface TreeData {
    // The flat nodes list used by react-accessible-treeview
    nodes: TreeNode[]

    // A map to quickly find the number ID for a node by its path
    pathToId: Map<string, number>

    // A set for node IDs that have been loaded. We can not rely on
    // children.length because a directory can have no children.
    loadedIds: Set<number>

    // The current path of the root node. An empty string for the root of the
    // tree.
    rootPath: string
}

function createTreeData(root: string): TreeData {
    return {
        nodes: [],
        pathToId: new Map(),
        loadedIds: new Set(),
        rootPath: root,
    }
}

function appendTreeData(
    tree: TreeData,
    entries: FileTreeEntry[],
    rootTreeUrl: string,
    alwaysLoadAncestors: boolean
): TreeData {
    tree = { ...tree, nodes: [...tree.nodes], pathToId: new Map(tree.pathToId) }

    // Insert a root node
    if (tree.nodes.length === 0) {
        insertRootNode(tree, rootTreeUrl, alwaysLoadAncestors)
    }

    let siblingCount = 0
    for (let idx = 0; idx < entries.length; idx++) {
        const entry = entries[idx]

        if (tree.pathToId.has(entry.path)) {
            continue
        }

        const isSiblingOfPreviousNode = idx > 0 && dirname(entries[idx - 1].path) === dirname(entry.path)
        if (isSiblingOfPreviousNode) {
            siblingCount++
        } else {
            siblingCount = 0
        }

        const id = tree.nodes.length
        const node: TreeNode = {
            name: entry.name,
            id,
            isBranch: entry.isDirectory,
            parent: 0,
            children: [],
            path: entry.path,
            entry,
            error: null,
            dotdot: null,
        }
        appendNode(tree, node)

        // We have reached the maximum number of entries for a directory. Add a
        // dummy node to indicate that there are more entries.
        if (siblingCount === MAX_TREE_ENTRIES - 1) {
            siblingCount = 0
            const errorMessage = 'Full list of files is too long to display. Use search to find a specific file.'
            const id = tree.nodes.length
            const node: TreeNode = {
                name: errorMessage,
                id,
                isBranch: false,
                parent: 0,
                children: [],
                // The path is only used to determine if a node is a parent of
                // another node so we can use dummy values.
                path: entry.path + '/...sourcegraph.error',
                entry: null,
                error: errorMessage,
                dotdot: null,
            }
            appendNode(tree, node)
        }
    }

    return tree
}

function insertRootNode(tree: TreeData, rootTreeUrl: string, alwaysLoadAncestors: boolean): void {
    const root: TreeNode = {
        name: tree.rootPath,
        id: 0,
        isBranch: true,
        parent: null,
        children: [],
        path: tree.rootPath,
        entry: null,
        error: null,
        dotdot: null,
    }
    tree.nodes.push(root)
    tree.pathToId.set(tree.rootPath, 0)

    if (!alwaysLoadAncestors && tree.rootPath !== '') {
        const id = tree.nodes.length
        const path = tree.rootPath + '/..'
        const node: TreeNode = {
            name: '..',
            id,
            isBranch: false,
            parent: 0,
            children: [],
            path,
            entry: null,
            error: null,
            dotdot: dirname(rootTreeUrl),
        }
        appendNode(tree, node)
    }
}

function appendNode(tree: TreeData, node: TreeNode): void {
    const children = tree.nodes.filter(potentialChild => isAParentOfB(node.path, potentialChild.path))
    const parent = tree.nodes.find(potentialParent => isAParentOfB(potentialParent.path, node.path))

    // Fix all children references
    node.children = children.map(child => child.id)
    for (const child of children) {
        child.parent = node.id
    }

    // Fix all parent references
    if (parent) {
        node.parent = parent.id
        parent.children.push(node.id)
    }

    // Only ordinary nodes should be added ot the pathToId map
    if (node.entry) {
        tree.pathToId.set(node.path, node.id)
    }

    tree.nodes.push(node)
}

function setLoadedPath(tree: TreeData, path: string): TreeData {
    tree = { ...tree, loadedIds: new Set(tree.loadedIds) }
    const id = tree.pathToId.get(path)
    if (!id) {
        throw new Error(`Node ${id} is not in the tree`)
    }
    tree.loadedIds.add(id)
    return tree
}

function isAParentOfB(fullPathA: string, fullPathB: string): boolean {
    // B is the root so it can not have a parent
    if (fullPathB === '') {
        return false
    }
    const bParentPath = fullPathB.split('/').slice(0, -1).join('/') || ''
    return fullPathA === bParentPath
}

function getAllParentsOfPath(tree: TreeData, fullPath: string): TreeNode[] {
    const parents = []

    let parent = tree.nodes.find(potentialParent => isAParentOfB(potentialParent.path, fullPath))
    while (parent) {
        parents.push(parent)

        parent = tree.nodes.find(potentialParent => isAParentOfB(potentialParent.path, parent!.path))
    }
    return parents
}
