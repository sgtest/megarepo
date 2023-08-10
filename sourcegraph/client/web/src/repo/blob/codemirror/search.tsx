/**
 * This extension extends CodeMirror's own search extension with a custom search
 * UI.
 */

import {
    findNext,
    findPrevious,
    getSearchQuery,
    openSearchPanel,
    search as codemirrorSearch,
    searchKeymap,
    SearchQuery,
    setSearchQuery,
} from '@codemirror/search'
import {
    Compartment,
    type Extension,
    StateEffect,
    type TransactionSpec,
    type Text as CodeMirrorText,
    type SelectionRange,
} from '@codemirror/state'
import {
    EditorView,
    type KeyBinding,
    keymap,
    type Panel,
    runScopeHandlers,
    ViewPlugin,
    type ViewUpdate,
} from '@codemirror/view'
import { mdiChevronDown, mdiChevronUp, mdiFormatLetterCase, mdiInformationOutline, mdiRegex } from '@mdi/js'
import { createRoot, type Root } from 'react-dom/client'
import type { NavigateFunction } from 'react-router-dom'
import { Subject, Subscription } from 'rxjs'
import { debounceTime, distinctUntilChanged, startWith, tap } from 'rxjs/operators'

import { QueryInputToggle } from '@sourcegraph/branded'
import { Toggle } from '@sourcegraph/branded/src/components/Toggle'
import { pluralize } from '@sourcegraph/common'
import { createUpdateableField } from '@sourcegraph/shared/src/components/CodeMirrorEditor'
import { shortcutDisplayName } from '@sourcegraph/shared/src/keyboardShortcuts'
import { Button, Icon, Input, Label, Text, Tooltip } from '@sourcegraph/wildcard'

import { Keybindings } from '../../../components/KeyboardShortcutsHelp/KeyboardShortcutsHelp'
import { createElement } from '../../../util/dom'

import { blobPropsFacet } from '.'
import { CodeMirrorContainer } from './react-interop'

const searchKeybinding = <Keybindings keybindings={[{ held: ['Mod'], ordered: ['F'] }]} />

const platformKeycombo = shortcutDisplayName('Mod+F')
const tooltipContent = `When enabled, ${platformKeycombo} searches the file only. Disable to search the page, and press ${platformKeycombo} for changes to apply.`
const searchKeybindingTooltip = (
    <Tooltip content={tooltipContent}>
        <Icon
            className="cm-sg-search-info ml-1 align-textbottom"
            svgPath={mdiInformationOutline}
            aria-label="Search keybinding information"
        />
    </Tooltip>
)

// Match 'from' position -> 1-based serial number (index) of this match in the document.
type SearchMatches = Map<number, number>

export const BLOB_SEARCH_CONTAINER_ID = 'blob-search-container'

const focusSearchInput = StateEffect.define<boolean>()

interface SearchPanelState {
    searchQuery: SearchQuery
    // The input value is usually derived from searchQuery. But we are
    // debouncing updating the searchQuery and without tracking the input value
    // separately user input would be lossing characters and feel laggy.
    inputValue: string
    overrideBrowserSearch: boolean
    matches: SearchMatches
    // Currently selected 1-based match index.
    currentMatchIndex: number | null
}

class SearchPanel implements Panel {
    public dom: HTMLElement
    public top = true

    private state: SearchPanelState
    private root: Root | null = null
    private input: HTMLInputElement | null = null
    private searchTerm = new Subject<string>()
    private subscriptions = new Subscription()
    private navigate: NavigateFunction

    constructor(private view: EditorView) {
        this.dom = createElement('div', {
            className: 'cm-sg-search-container d-flex align-items-center',
            id: BLOB_SEARCH_CONTAINER_ID,
            onkeydown: this.onkeydown,
        })
        this.navigate = view.state.facet(blobPropsFacet).navigate

        const searchQuery = getSearchQuery(this.view.state)
        const matches = calculateMatches(searchQuery, view.state.doc)
        this.state = {
            searchQuery,
            inputValue: searchQuery.search,
            overrideBrowserSearch: this.view.state.field(overrideBrowserFindInPageShortcut),
            matches,
            currentMatchIndex: getMatchIndexForSelection(matches, view.state.selection.main),
        }

        this.subscriptions.add(
            this.searchTerm
                .pipe(
                    startWith(this.state.searchQuery.search),
                    distinctUntilChanged(),
                    // Immediately update input for fast feedback
                    tap(value => {
                        this.state = { ...this.state, inputValue: value }
                        this.render(this.state)
                    }),
                    debounceTime(100)
                )
                .subscribe(searchTerm => this.commit({ search: searchTerm }))
        )
    }

    public update(update: ViewUpdate): void {
        let newState = this.state

        const searchQuery = getSearchQuery(update.state)
        const searchQueryChanged = !searchQuery.eq(this.state.searchQuery)
        if (searchQueryChanged) {
            newState = {
                ...newState,
                inputValue: searchQuery.search,
                searchQuery,
                matches: calculateMatches(searchQuery, update.view.state.doc),
            }
        }

        const overrideBrowserSearch = update.state.field(overrideBrowserFindInPageShortcut)
        if (overrideBrowserSearch !== this.state.overrideBrowserSearch) {
            newState = { ...newState, overrideBrowserSearch }
        }

        // It looks like update.SelectionSet is not set when the search query changes
        if (searchQueryChanged || update.selectionSet) {
            newState = {
                ...newState,
                currentMatchIndex: getMatchIndexForSelection(newState.matches, update.view.state.selection.main),
            }
        }

        if (newState !== this.state) {
            this.state = newState
            this.render(this.state)
        }

        if (
            update.transactions.some(transaction =>
                transaction.effects.some(effect => effect.is(focusSearchInput) && effect.value)
            )
        ) {
            this.input?.focus()
            this.input?.select()
        }
    }

    public mount(): void {
        this.render(this.state)
    }

    public destroy(): void {
        this.subscriptions.unsubscribe()
    }

    private render({
        searchQuery,
        inputValue,
        overrideBrowserSearch,
        currentMatchIndex,
        matches,
    }: SearchPanelState): void {
        if (!this.root) {
            this.root = createRoot(this.dom)
        }

        const totalMatches = matches.size

        this.root.render(
            <CodeMirrorContainer
                navigate={this.navigate}
                onMount={() => {
                    this.input?.focus()
                    this.input?.select()
                }}
            >
                <div className="cm-sg-search-input d-flex align-items-center pr-2 mr-2">
                    <Input
                        ref={element => (this.input = element)}
                        type="search"
                        name="search"
                        variant="small"
                        placeholder="Find..."
                        autoComplete="off"
                        inputClassName={searchQuery.search && totalMatches === 0 ? 'text-danger' : ''}
                        value={inputValue}
                        onChange={event => this.searchTerm.next(event.target.value)}
                        main-field="true"
                    />
                    <QueryInputToggle
                        isActive={searchQuery.caseSensitive}
                        onToggle={() => this.commit({ caseSensitive: !searchQuery.caseSensitive })}
                        iconSvgPath={mdiFormatLetterCase}
                        title="Case sensitivity"
                        className="test-blob-view-search-case-sensitive"
                    />
                    <QueryInputToggle
                        isActive={searchQuery.regexp}
                        onToggle={() => this.commit({ regexp: !searchQuery.regexp })}
                        iconSvgPath={mdiRegex}
                        title="Regular expression"
                        className="test-blob-view-search-regexp"
                    />
                </div>
                <Button
                    className="mr-2"
                    type="button"
                    size="sm"
                    outline={true}
                    variant="secondary"
                    onClick={this.findPrevious}
                    data-testid="blob-view-search-previous"
                >
                    <Icon svgPath={mdiChevronUp} aria-hidden={true} />
                    Previous
                </Button>

                <Button
                    className="mr-3"
                    type="button"
                    size="sm"
                    outline={true}
                    variant="secondary"
                    onClick={this.findNext}
                    data-testid="blob-view-search-next"
                >
                    <Icon svgPath={mdiChevronDown} aria-hidden={true} />
                    Next
                </Button>

                {searchQuery.search ? (
                    <div>
                        <Text className="m-0">
                            {currentMatchIndex !== null && `${currentMatchIndex} / `}
                            {totalMatches} {pluralize('result', totalMatches)}
                        </Text>
                    </div>
                ) : null}

                <div className="ml-auto">
                    <Label className="mb-0">
                        <Toggle
                            className="mr-1 align-text-bottom"
                            value={overrideBrowserSearch}
                            onToggle={this.setOverrideBrowserSearch}
                        />
                        {searchKeybinding} searches file
                    </Label>
                    {searchKeybindingTooltip}
                </div>
            </CodeMirrorContainer>
        )
    }

    private setOverrideBrowserSearch = (override: boolean): void =>
        this.view.dispatch({
            effects: setOverrideBrowserFindInPageShortcut.of(override),
        })

    private findNext = (): void => {
        findNext(this.view)
    }

    private findPrevious = (): void => {
        findPrevious(this.view)
    }

    // Taken from CodeMirror's default search panel implementation. This is
    // necessary so that pressing Meta+F (and other CodeMirror keybindings) will
    // trigger the configured event handlers and not just fall back to the
    // browser's default behavior.
    private onkeydown = (event: KeyboardEvent): void => {
        if (runScopeHandlers(this.view, event, 'search-panel')) {
            event.preventDefault()
        } else if (event.code === 'Enter' && event.target === this.input) {
            event.preventDefault()
            if (event.shiftKey) {
                this.findPrevious()
            } else {
                this.findNext()
            }
        }
    }

    private commit = ({
        search,
        caseSensitive,
        regexp,
    }: {
        search?: string
        caseSensitive?: boolean
        regexp?: boolean
    }): void => {
        const query = new SearchQuery({
            search: search ?? this.state.searchQuery.search,
            caseSensitive: caseSensitive ?? this.state.searchQuery.caseSensitive,
            regexp: regexp ?? this.state.searchQuery.regexp,
        })

        if (!query.eq(this.state.searchQuery)) {
            let transactionSpec: TransactionSpec = {}
            const effects: StateEffect<any>[] = [setSearchQuery.of(query)]

            if (query.search) {
                // The following code scrolls next match into view if there is no
                // match in the visible viewport. This is done by searching for the
                // text from the currently top visible line and determining whether
                // the next match is in the current viewport

                const { scrollTop } = this.view.scrollDOM

                // Get top visible line. More than half of the line must be visible.
                // We don't use `view.viewportLineBlocks` because that also includes
                // lines that are rendered but not actually visible.
                let topLineBlock = this.view.lineBlockAtHeight(scrollTop)
                if (Math.abs(topLineBlock.bottom - scrollTop) <= topLineBlock.height / 2) {
                    topLineBlock = this.view.lineBlockAtHeight(scrollTop + topLineBlock.height)
                }

                let result = query.getCursor(this.view.state.doc, topLineBlock.from).next()
                if (result.done) {
                    // No match in the remainder of the document, wrap around
                    result = query.getCursor(this.view.state.doc).next()
                }

                if (!result.done) {
                    // Taken from the original `findPrevious` and `findNext` CodeMirror implementation:
                    // https://github.com/codemirror/search/blob/affb772655bab706e08f99bd50a0717bfae795f5/src/search.ts#L385-L416

                    transactionSpec = {
                        selection: { anchor: result.value.from, head: result.value.to },
                        scrollIntoView: true,
                        userEvent: 'select.search',
                    }
                    effects.push(announceMatch(this.view, result.value))
                }
                // Search term is not in the document, nothing to do
            }

            this.view.dispatch({
                ...transactionSpec,
                effects,
            })
        }
    }
}

function calculateMatches(query: SearchQuery, document: CodeMirrorText): SearchMatches {
    const newSearchMatches: SearchMatches = new Map()
    let index = 1
    let result = query.getCursor(document).next()
    while (!result.done) {
        newSearchMatches.set(result.value.from, index++)
        result = query.getCursor(document, result.value.to).next()
    }
    return newSearchMatches
}

function getMatchIndexForSelection(matches: SearchMatches, range: SelectionRange): number | null {
    return range.empty ? null : matches.get(range.from) ?? null
}

// Announce the current match to screen readers.
// Taken from original the CodeMirror implementation:
// https://github.com/codemirror/search/blob/affb772655bab706e08f99bd50a0717bfae795f5/src/search.ts#L694-L717
const announceMargin = 30
const breakRegex = /[\s!,.:;?]/
function announceMatch(view: EditorView, { from, to }: { from: number; to: number }): StateEffect<string> {
    const line = view.state.doc.lineAt(from)
    const lineEnd = view.state.doc.lineAt(to).to
    const start = Math.max(line.from, from - announceMargin)
    const end = Math.min(lineEnd, to + announceMargin)
    let text = view.state.sliceDoc(start, end)
    if (start !== line.from) {
        for (let index = 0; index < announceMargin; index++) {
            if (!breakRegex.test(text[index + 1]) && breakRegex.test(text[index])) {
                text = text.slice(index)
                break
            }
        }
    }
    if (end !== lineEnd) {
        for (let index = text.length - 1; index > text.length - announceMargin; index--) {
            if (!breakRegex.test(text[index - 1]) && breakRegex.test(text[index])) {
                text = text.slice(0, index)
                break
            }
        }
    }

    return EditorView.announce.of(
        `${view.state.phrase('current match')}. ${text} ${view.state.phrase('on line')} ${line.number}.`
    )
}

const theme = EditorView.theme({
    '.cm-sg-search-container': {
        backgroundColor: 'var(--code-bg)',
        padding: '0.375rem 1rem',
    },
    '.cm-sg-search-input': {
        borderRadius: 'var(--border-radius)',
        border: '1px solid var(--input-border-color)',

        '&:focus-within': {
            borderColor: 'var(--inpt-focus-border-color)',
            boxShadow: 'var(--input-focus-box-shadow)',
        },

        '& input': {
            borderColor: 'transparent',
            '&:focus': {
                boxShadow: 'none',
            },
        },
    },
    '.search-container > input.form-control': {
        width: '15rem',
    },
    '.cm-searchMatch': {
        backgroundColor: 'var(--mark-bg)',
    },
    '.cm-searchMatch-selected': {
        backgroundColor: 'var(--oc-orange-3)',
    },
    '.cm-sg-search-info': {
        color: 'var(--gray-06)',
    },
})

interface SearchConfig {
    overrideBrowserFindInPageShortcut: boolean
    onOverrideBrowserFindInPageToggle: (enabled: boolean) => void
}

const [overrideBrowserFindInPageShortcut, , setOverrideBrowserFindInPageShortcut] = createUpdateableField(true)

export function search(config: SearchConfig): Extension {
    const keymapCompartment = new Compartment()

    function getKeyBindings(override: boolean): readonly KeyBinding[] {
        if (override) {
            return searchKeymap.map(binding =>
                binding.key === 'Mod-f'
                    ? {
                          ...binding,
                          run: view => {
                              // By default pressing Mod+f when the search input is already focused won't select
                              // the input value, unlike browser's built-in search feature.
                              // We are overwriting the keybinding here to ensure that the input value is always
                              // selected.
                              const result = binding.run?.(view)
                              if (result) {
                                  view.dispatch({ effects: focusSearchInput.of(true) })
                                  return true
                              }
                              return false
                          },
                      }
                    : binding
            )
        }
        return searchKeymap.filter(binding => binding.key !== 'Mod-f' && binding.key !== 'Escape')
    }

    return [
        overrideBrowserFindInPageShortcut.init(() => config.overrideBrowserFindInPageShortcut),
        EditorView.updateListener.of(update => {
            const override = update.state.field(overrideBrowserFindInPageShortcut)
            if (update.startState.field(overrideBrowserFindInPageShortcut) !== override) {
                config.onOverrideBrowserFindInPageToggle(override)
                update.view.dispatch({ effects: keymapCompartment.reconfigure(keymap.of(getKeyBindings(override))) })
            }
        }),
        theme,
        keymapCompartment.of(keymap.of(getKeyBindings(config.overrideBrowserFindInPageShortcut))),
        codemirrorSearch({
            createPanel: view => new SearchPanel(view),
        }),
        ViewPlugin.define(view => {
            if (!config.overrideBrowserFindInPageShortcut) {
                window.requestAnimationFrame(() => openSearchPanel(view))
            }
            return {}
        }),
    ]
}
