import type * as Comlink from 'comlink'
import { EMPTY, of } from 'rxjs'
import { first, switchMap } from 'rxjs/operators'
import type * as vscode from 'vscode'

import { finallyReleaseProxy, wrapRemoteObservable } from '@sourcegraph/shared/src/api/client/api/common'
import { makeRepoURI, parseRepoURI } from '@sourcegraph/shared/src/util/url'

import type { SearchSidebarAPI } from '../contract'
import type { SourcegraphFileSystemProvider } from '../file-system/SourcegraphFileSystemProvider'

export class SourcegraphDefinitionProvider implements vscode.DefinitionProvider {
    constructor(
        private readonly fs: SourcegraphFileSystemProvider,
        private readonly sourcegraphExtensionHostAPI: Comlink.Remote<SearchSidebarAPI>
    ) {}
    public async provideDefinition(
        document: vscode.TextDocument,
        position: vscode.Position,
        token: vscode.CancellationToken
    ): Promise<vscode.Definition | undefined> {
        const uri = this.fs.sourcegraphUri(document.uri)
        const extensionHostUri = makeRepoURI({
            repoName: uri.repositoryName,
            revision: uri.revision,
            filePath: uri.path,
        })

        const definitions = wrapRemoteObservable(
            this.sourcegraphExtensionHostAPI.getDefinition({
                textDocument: {
                    uri: extensionHostUri,
                },
                position: {
                    line: position.line,
                    character: position.character,
                },
            })
        )
            .pipe(
                finallyReleaseProxy(),
                switchMap(({ isLoading, result }) => {
                    if (isLoading) {
                        return EMPTY
                    }

                    const locations = result.map(location => {
                        const uri = parseRepoURI(location.uri)

                        return this.fs.toVscodeLocation({
                            resource: {
                                path: uri.filePath ?? '',
                                repositoryName: uri.repoName,
                                revision: uri.commitID ?? uri.revision ?? '',
                            },
                            range: location.range,
                        })
                    })

                    return of(locations)
                }),
                first()
            )
            .toPromise()

        token.onCancellationRequested(() => {
            // Debt: manually create promise so we can cancel request.
        })

        return definitions
    }
}
