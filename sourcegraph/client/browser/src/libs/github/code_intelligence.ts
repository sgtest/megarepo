import { AdjustmentDirection, PositionAdjuster } from '@sourcegraph/codeintellify'
import { trimStart } from 'lodash'
import { map } from 'rxjs/operators'
import { FileSpec, RepoSpec, ResolvedRevSpec, RevSpec } from '../../../../../shared/src/util/url'
import { JumpURLLocation } from '../../shared/backend/lsp'
import { fetchBlobContentLines } from '../../shared/repo/backend'
import { CodeHost, CodeView, CodeViewResolver, CodeViewWithOutSelector } from '../code_intelligence'
import {
    diffDomFunctions,
    getDiffLineRanges,
    getLineRanges,
    searchCodeSnippetDOMFunctions,
    singleFileDOMFunctions,
} from './dom_functions'
import { getCommandPaletteMount, getGlobalDebugMount } from './extensions'
import { resolveDiffFileInfo, resolveFileInfo, resolveSnippetFileInfo } from './file_info'
import { createCodeViewToolbarMount, getFileContainers, parseURL } from './util'

const toolbarButtonProps = {
    className: 'btn btn-sm tooltipped tooltipped-n',
    style: { marginRight: '5px', textDecoration: 'none', color: 'inherit' },
}

const diffCodeView: CodeViewWithOutSelector = {
    dom: diffDomFunctions,
    getToolbarMount: createCodeViewToolbarMount,
    resolveFileInfo: resolveDiffFileInfo,
    toolbarButtonProps,
    getLineRanges: getDiffLineRanges,
    isDiff: true,
}

const diffConversationCodeView: CodeViewWithOutSelector = {
    ...diffCodeView,
    getToolbarMount: undefined,
}

const singleFileCodeView: CodeViewWithOutSelector = {
    dom: singleFileDOMFunctions,
    getToolbarMount: createCodeViewToolbarMount,
    resolveFileInfo,
    toolbarButtonProps,
    getLineRanges,
    isDiff: false,
}

/**
 * Some code snippets get leading white space trimmed. This adjusts based on
 * this. See an example here https://github.com/sourcegraph/browser-extensions/issues/188.
 */
const adjustPositionForSnippet: PositionAdjuster<RepoSpec & RevSpec & FileSpec & ResolvedRevSpec> = ({
    direction,
    codeView,
    position,
}) =>
    fetchBlobContentLines(position).pipe(
        map(lines => {
            const codeElement = singleFileDOMFunctions.getCodeElementFromLineNumber(
                codeView,
                position.line,
                position.part
            )
            if (!codeElement) {
                throw new Error('(adjustPosition) could not find code element for line provided')
            }

            const actualLine = lines[position.line - 1]
            const documentLine = codeElement.textContent || ''

            const actualLeadingWhiteSpace = actualLine.length - trimStart(actualLine).length
            const documentLeadingWhiteSpace = documentLine.length - trimStart(documentLine).length

            const modifier = direction === AdjustmentDirection.ActualToCodeView ? -1 : 1
            const delta = Math.abs(actualLeadingWhiteSpace - documentLeadingWhiteSpace) * modifier

            return {
                line: position.line,
                character: position.character + delta,
            }
        })
    )

const searchResultCodeView: CodeView = {
    selector: '.code-list-item',
    dom: searchCodeSnippetDOMFunctions,
    adjustPosition: adjustPositionForSnippet,
    resolveFileInfo: resolveSnippetFileInfo,
    toolbarButtonProps,
    getLineRanges,
    isDiff: false,
}

const commentSnippetCodeView: CodeView = {
    selector: '.js-comment-body',
    dom: singleFileDOMFunctions,
    resolveFileInfo: resolveSnippetFileInfo,
    adjustPosition: adjustPositionForSnippet,
    toolbarButtonProps,
    getLineRanges,
    isDiff: false,
}

const resolveCodeView = (elem: HTMLElement): CodeViewWithOutSelector | null => {
    if (elem.querySelector('article.markdown-body')) {
        // This code view is rendered markdown, we shouldn't add code intelligence
        return null
    }

    const files = document.getElementsByClassName('file')
    const { filePath } = parseURL()
    const isSingleCodeFile = files.length === 1 && filePath && document.getElementsByClassName('diff-view').length === 0

    if (isSingleCodeFile) {
        return singleFileCodeView
    }

    if (elem.closest('.discussion-item-body')) {
        return diffConversationCodeView
    }

    return diffCodeView
}

const codeViewResolver: CodeViewResolver = {
    selector: '.file',
    resolveCodeView,
}

function checkIsGithub(): boolean {
    const href = window.location.href

    const isGithub = /^https?:\/\/(www.)?github.com/.test(href)
    const ogSiteName = document.head!.querySelector(`meta[property='og:site_name']`) as HTMLMetaElement
    const isGitHubEnterprise = ogSiteName ? ogSiteName.content === 'GitHub Enterprise' : false

    return isGithub || isGitHubEnterprise
}

const getOverlayMount = () => {
    const container = document.querySelector('#js-repo-pjax-container')
    if (!container) {
        throw new Error('unable to find repo pjax container')
    }

    const mount = document.createElement('div')
    container.appendChild(mount)

    return mount
}

export const githubCodeHost: CodeHost = {
    name: 'github',
    codeViews: [searchResultCodeView, commentSnippetCodeView],
    codeViewResolver,
    check: checkIsGithub,
    getOverlayMount,
    getCommandPaletteMount,
    getGlobalDebugMount,
    buildJumpURLLocation: (def: JumpURLLocation) => {
        const rev = def.rev || 'HEAD'
        // If we're provided options, we can make the j2d URL more specific.
        const { repoName } = parseURL()

        const sameRepo = repoName === def.repoName
        // Stay on same page in PR if possible.
        if (sameRepo && def.part) {
            const containers = getFileContainers()
            for (const container of containers) {
                const header = container.querySelector('.file-header') as HTMLElement
                const anchorPath = header.dataset.path
                if (anchorPath === def.filePath) {
                    const anchorUrl = header.dataset.anchor
                    const url = `${window.location.origin}${window.location.pathname}#${anchorUrl}${
                        def.part === 'base' ? 'L' : 'R'
                    }${def.position.line}`

                    return url
                }
            }
        }

        return `https://${def.repoName}/blob/${rev}/${def.filePath}#L${def.position.line}${
            def.position.character ? ':' + def.position.character : ''
        }`
    },
}
