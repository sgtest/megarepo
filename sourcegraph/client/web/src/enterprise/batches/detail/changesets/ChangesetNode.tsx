import * as H from 'history'
import React from 'react'

import { Hoverifier } from '@sourcegraph/codeintellify'
import { ActionItemAction } from '@sourcegraph/shared/src/actions/ActionItem'
import { HoverMerged } from '@sourcegraph/shared/src/api/client/types/hover'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { RepoSpec, RevisionSpec, FileSpec, ResolvedRevisionSpec } from '@sourcegraph/shared/src/util/url'

import { ChangesetFields } from '../../../../graphql-operations'
import { queryExternalChangesetWithFileDiffs } from '../backend'

import styles from './ChangesetNode.module.scss'
import { ExternalChangesetNode } from './ExternalChangesetNode'
import { HiddenExternalChangesetNode } from './HiddenExternalChangesetNode'

export interface ChangesetNodeProps extends ThemeProps {
    node: ChangesetFields
    viewerCanAdminister: boolean
    history: H.History
    location: H.Location
    onSelect?: (id: string, selected: boolean) => void
    isSelected?: (id: string) => boolean
    extensionInfo?: {
        hoverifier: Hoverifier<RepoSpec & RevisionSpec & FileSpec & ResolvedRevisionSpec, HoverMerged, ActionItemAction>
    } & ExtensionsControllerProps
    /** For testing purposes. */
    queryExternalChangesetWithFileDiffs?: typeof queryExternalChangesetWithFileDiffs
    /** For testing purposes. */
    expandByDefault?: boolean
}

export const ChangesetNode: React.FunctionComponent<ChangesetNodeProps> = ({ node, ...props }) => {
    if (node.__typename === 'ExternalChangeset') {
        return (
            <>
                <span className={styles.changesetNodeSeparator} />
                <ExternalChangesetNode node={node} {...props} />
            </>
        )
    }
    return (
        <>
            <span className={styles.changesetNodeSeparator} />
            <HiddenExternalChangesetNode node={node} {...props} />
        </>
    )
}
