import React from 'react'

import { snippet } from '@codemirror/autocomplete'
import {
    EditorSelection,
    type EditorState,
    type Extension,
    Facet,
    Prec,
    StateEffect,
    StateField,
    type Transaction,
} from '@codemirror/state'
import {
    type Command as CodeMirrorCommand,
    EditorView,
    type KeyBinding,
    keymap,
    ViewPlugin,
    type ViewUpdate,
} from '@codemirror/view'
import { createRoot, type Root } from 'react-dom/client'

import { compatNavigate, type HistoryOrNavigate } from '@sourcegraph/common'

import { getSelectedMode, modeChanged, modesFacet, setModeEffect } from './modes'
import { Suggestions } from './Suggestions'

const ASYNC_THROTTLE_TIME = 300

/**
 * A source for completion/suggestion results
 */
export interface Source {
    query: (state: EditorState, position: number, mode?: string) => SuggestionResult | null
    mode?: string
}

export interface SuggestionResult {
    /**
     * Initial/synchronous result.
     */
    result: Group[]
    /**
     * Function to be called to load additional results if necessary.
     */
    next?: () => Promise<SuggestionResult>
    /**
     * Determines whether this result is invalidated by the new editor state.
     */
    valid?: (state: EditorState, position: number) => boolean
}

export type CustomRenderer<T> = ((value: T) => React.ReactElement) | string

export interface Option {
    /**
     * The label the input is matched against and shown in the UI.
     */
    label: string
    /**
     * What to do when this option is applied (via Enter)
     */
    action: Action
    /**
     * Options can have perform an alternative action when applied via
     * Mod-Enter.
     */
    alternativeAction?: Action
    /**
     * A short description of the option, shown next to the label.
     */
    description?: string
    /**
     * The SVG path of the icon to use for this option.
     */
    icon?: string
    /**
     * If present the provided component will be used to render the label of the
     * option.
     */
    render?: CustomRenderer<Option>
    /**
     * If present this component is rendered as footer.
     */
    info?: CustomRenderer<Option>
    /**
     * A set of character indexes. If provided the characters of at these
     * positions in the label will be highlighted as matches.
     */
    matches?: Set<number>
    /**
     * A word that describes the nature of this option (e.g. file, repo, ...)
     * Not used by the suggestion engine, but possibly used for metrics collection.
     */
    kind: string
}

export interface CommandAction {
    type: 'command'
    apply: (option: Option, view: EditorView) => void
    name?: string
    /**
     * If present this component is rendered as part of the footer.
     */
    info?: CustomRenderer<Action>
}
export interface GoToAction {
    type: 'goto'
    url: string
    name?: string
    /**
     * If present this component is rendered as part of the footer.
     */
    info?: CustomRenderer<Action>
}
export interface CompletionAction {
    type: 'completion'
    from: number
    name?: string
    to?: number
    insertValue?: string
    /**
     * If present this component is rendered as part of the footer.
     */
    info?: CustomRenderer<Action>
    asSnippet?: boolean
}
export type Action = CommandAction | GoToAction | CompletionAction

export interface Group {
    title: string
    options: Option[]
}

class SuggestionView {
    private container: HTMLDivElement
    private root: Root

    private onSelect = (option: Option, action?: Action): void => {
        applyAction(this.view, action ?? option.action, option, 'mouse')
        // Query input looses focus when option is selected via
        // mousedown/click. This is a necessary hack to re-focus the query
        // input.
        window.requestAnimationFrame(() => this.view.contentDOM.focus())
    }

    constructor(private readonly id: string, public readonly view: EditorView, public parent: HTMLDivElement) {
        const state = view.state.field(suggestionsStateField)
        this.container = document.createElement('div')
        this.root = createRoot(this.container)
        parent.append(this.container)

        // We need to delay the initial render otherwise React complains that
        // wer are rendering a component while already rendering another one
        // (the query input component)
        setTimeout(() => {
            this.root.render(
                React.createElement(Suggestions, {
                    id,
                    results: state.result.groups,
                    activeRowIndex: state.selectedOption,
                    open: state.open,
                    onSelect: this.onSelect,
                })
            )
        }, 0)
    }

    public update(update: ViewUpdate): void {
        const state = update.state.field(suggestionsStateField)

        if (state !== update.startState.field(suggestionsStateField)) {
            this.updateResults(state)
        }
    }

    private updateResults(state: SuggestionsState): void {
        this.root.render(
            React.createElement(Suggestions, {
                id: this.id,
                results: state.result.groups,
                activeRowIndex: state.selectedOption,
                open: state.open,
                onSelect: this.onSelect,
            })
        )
    }

    public destroy(): void {
        this.container.remove()

        // We need to delay unmounting the root otherwise React complains about
        // synchronsouly unmounting multiple components.
        setTimeout(() => {
            this.root.unmount()
        }, 0)
    }
}

/**
 * This plugin is responsible for executing async queries.
 */
const completionPlugin = ViewPlugin.fromClass(
    class {
        private next: Query | null = null
        private timer: number | null = null

        constructor(public readonly view: EditorView) {
            this.maybeScheduleRun(view.state.field(suggestionsStateField).query)
        }

        public update(update: ViewUpdate): void {
            const source = update.state.field(suggestionsStateField).query

            if (update.view.hasFocus && source !== update.startState.field(suggestionsStateField).query) {
                this.maybeScheduleRun(source)
            }
        }

        /**
         * Implements a throttle mechanism. If no timer is running we execute the query
         * immediately and start a timer. When the timer expires we run the last query that
         * has arrived in the meantime.
         * If a timer is running we keep track of the next query that should be run.
         */
        private maybeScheduleRun(query: Query): void {
            // If the source is not in a pending state we clear any possibly
            // ongoing request
            if (!query.isPending()) {
                this.next = null
                if (this.timer !== null) {
                    window.clearTimeout(this.timer)
                }
                this.timer = null
                return
            }

            if (this.timer) {
                // Request is already in progress, schedule a new one for the
                // next "tick"
                this.next = query
            } else {
                this.next = null
                query
                    .fetch()
                    .then(result => this.view.dispatch({ effects: updateResultEffect.of({ query, result }) }))
                    .catch(() => {})
                this.timer = window.setTimeout(() => {
                    this.timer = null
                    if (this.next) {
                        this.maybeScheduleRun(this.next)
                    }
                }, ASYNC_THROTTLE_TIME)
            }
        }
    }
)

/**
 * Wrapper class to make operating on groups of options easier.
 */
class Result {
    private allOptions: Option[]

    constructor(
        public readonly groups: Group[],
        public valid: (state: EditorState, position: number) => boolean = () => false
    ) {
        this.allOptions = groups.flatMap(group => group.options)
    }

    public static fromSuggestionResult(result: SuggestionResult): Result {
        return new Result(result.result, result.valid)
    }

    // eslint-disable-next-line id-length
    public at(index: number): Option | undefined {
        return this.allOptions[index]
    }

    public groupRowIndex(index: number): [number, number] | undefined {
        const option = this.allOptions[index]

        if (!option) {
            return undefined
        }

        for (let groupIndex = 0; groupIndex < this.groups.length; groupIndex++) {
            const options = this.groups[groupIndex].options
            for (let rowIndex = 0; rowIndex < options.length; rowIndex++) {
                if (options[rowIndex] === option) {
                    return [groupIndex, rowIndex]
                }
            }
        }

        return undefined
    }

    public groupFor(option: Option): Group | undefined {
        return this.groups.find(group => group.options.includes(option))
    }

    public empty(): boolean {
        return this.length === 0
    }

    public get length(): number {
        return this.allOptions.length
    }
}

const emptyResult = new Result([])

enum QueryState {
    Inactive,
    Pending,
    Complete,
}

/**
 * Wrapper around the configered sources. Keeps track of the state and results.
 */
class Query {
    constructor(
        public readonly sources: readonly Source[],
        public readonly state: QueryState,
        public readonly result: Result
    ) {}

    public update(transaction: Transaction): Query {
        // Aliasing this makes it easier to create new instances based on all
        // changes and effects of the transaction.
        // eslint-disable-next-line @typescript-eslint/no-this-alias, unicorn/no-this-assignment
        let query: Query = this

        if (isUserInput(transaction) || transaction.docChanged || modeChanged(transaction)) {
            query = query.run(transaction.state)
        } else if (transaction.selection) {
            if (!transaction.selection.main.empty) {
                // Hide suggestions when the user selects a range in the input
                query = query.updateWithState(QueryState.Inactive)
            } else if (!query.result.valid(transaction.state, transaction.newSelection.main.head)) {
                query = query.run(transaction.state)
            }
        }

        for (const effect of transaction.effects) {
            // Only "apply" the effect if the results are for the curent query. This prevents
            // overwriting the results from stale requests.
            if (effect.is(updateResultEffect) && effect.value.query === query) {
                query = query.updateWithSuggestionResult(effect.value.result)
            } else if (effect.is(startCompletionEffect)) {
                query = query.run(transaction.state)
            } else if (effect.is(hideCompletionEffect)) {
                query = query.updateWithState(QueryState.Inactive)
            }
        }

        return query
    }

    private run(state: EditorState): Query {
        const selectedMode = getSelectedMode(state)
        const activeSources = this.sources.filter(source => source.mode === selectedMode?.name)
        const result = combineResults(
            activeSources.map(source => source.query(state, state.selection.main.head, selectedMode?.name))
        )
        return this.updateWithSuggestionResult(result)
    }

    private updateWithSuggestionResult(result: SuggestionResult): Query {
        return result.next
            ? new PendingQuery(this.sources, Result.fromSuggestionResult(result), result.next)
            : new Query(this.sources, QueryState.Complete, Result.fromSuggestionResult(result))
    }

    private updateWithState(state: QueryState.Inactive | QueryState.Complete): Query {
        return state !== this.state ? new Query(this.sources, state, this.result) : this
    }

    public isInactive(): boolean {
        return this.state === QueryState.Inactive
    }

    public isPending(): this is PendingQuery {
        return this.state === QueryState.Pending
    }
}

class PendingQuery extends Query {
    constructor(
        public readonly sources: readonly Source[],
        public readonly result: Result,
        public readonly fetch: () => Promise<SuggestionResult>
    ) {
        super(sources, QueryState.Pending, result)
    }
}

/**
 * Takes multiple suggestion results and combines the groups of each of them.
 * The order of items within a group is determined by the order of results.
 */
export function combineResults(results: (SuggestionResult | null)[]): SuggestionResult {
    const options: Record<Group['title'], Group['options'][]> = {}
    let hasValid = false
    let hasNext = false

    for (const result of results) {
        if (!result) {
            continue
        }
        for (const group of result.result) {
            if (!options[group.title]) {
                options[group.title] = []
            }
            options[group.title].push(group.options)
        }
        if (result.next) {
            hasNext = true
        }
        if (result.valid) {
            hasValid = true
        }
    }

    const staticResult: SuggestionResult = {
        result: Object.entries(options).map(([title, options]) => ({ title, options: options.flat() })),
    }

    if (hasValid) {
        staticResult.valid = (...args) => results.every(result => result?.valid?.(...args) ?? false)
    }
    if (hasNext) {
        staticResult.next = () => Promise.all(results.map(result => result?.next?.() ?? result)).then(combineResults)
    }

    return staticResult
}

/**
 * Main suggestions state. Mangages of data source and selected option.
 */
class SuggestionsState {
    constructor(public readonly query: Query, public readonly open: boolean, public readonly selectedOption: number) {}

    public update(transaction: Transaction): SuggestionsState {
        // Aliasing makes it easier to update the state
        // eslint-disable-next-line @typescript-eslint/no-this-alias,unicorn/no-this-assignment
        let state: SuggestionsState = this

        const sources = transaction.state.facet(suggestionSources)
        let query = sources === state.query.sources ? state.query : new Query(sources, QueryState.Inactive, emptyResult)
        query = query.update(transaction)
        if (query !== state.query) {
            state = new SuggestionsState(
                query,
                !query.isInactive(),
                // Preserve the currently selected option if the query was pending
                // (ensures that the selected option doesn't change when new options become available)
                state.query.isPending() ? state.selectedOption : -1
            )
        }

        if (state.selectedOption > -1 && transaction.newDoc.length === 0) {
            state = new SuggestionsState(state.query, !state.query.isInactive(), -1)
        }

        for (const effect of transaction.effects) {
            if (effect.is(setSelectedEffect)) {
                state = new SuggestionsState(state.query, state.open, effect.value)
            }
            if (effect.is(hideCompletionEffect)) {
                state = new SuggestionsState(state.query, false, state.selectedOption)
            }
        }

        return state
    }

    public get result(): Result {
        return this.query.result
    }
}

function isUserInput(transaction: Transaction): boolean {
    return (
        transaction.isUserEvent('input.type') ||
        transaction.isUserEvent('input.paste') ||
        transaction.isUserEvent('delete')
    )
}

interface Config {
    id: string
    historyOrNavigate?: HistoryOrNavigate
}

const suggestionsConfig = Facet.define<Config, Config>({
    combine(configs) {
        return configs[0] ?? {}
    },
})

const setSelectedEffect = StateEffect.define<number>()
const startCompletionEffect = StateEffect.define<void>()
const hideCompletionEffect = StateEffect.define<void>()
const updateResultEffect = StateEffect.define<{ query: Query; result: SuggestionResult }>()
const suggestionsStateField = StateField.define<SuggestionsState>({
    create() {
        return new SuggestionsState(new Query([], QueryState.Inactive, emptyResult), false, -1)
    },

    update(state, transaction) {
        return state.update(transaction)
    },

    provide(field) {
        return EditorView.contentAttributes.compute([field, suggestionsConfig], state => {
            const id = state.facet(suggestionsConfig).id
            const suggestionState = state.field(field)
            const groupRowIndex = suggestionState.result.groupRowIndex(suggestionState.selectedOption)
            return {
                'aria-expanded': suggestionState.open && !suggestionState.result.empty() ? 'true' : 'false',
                'aria-activedescendant': groupRowIndex ? `${id}-${groupRowIndex[0]}x${groupRowIndex[1]}` : '',
            }
        })
    },
})

function moveSelection(direction: 'forward' | 'backward'): CodeMirrorCommand {
    const forward = direction === 'forward'
    return view => {
        const state = view.state.field(suggestionsStateField, false)
        if (!state?.open || state.result.empty()) {
            return false
        }
        const { length } = state?.result
        let selected = state.selectedOption > -1 ? state.selectedOption + (forward ? 1 : -1) : forward ? 0 : length - 1
        if (selected < 0) {
            selected = length - 1
        } else if (selected >= length) {
            selected = 0
        }
        view.dispatch({ effects: setSelectedEffect.of(selected) })
        return true
    }
}

function applyAction(view: EditorView, action: Action, option: Option, source: SelectionSource): void {
    switch (action.type) {
        case 'completion':
            {
                const to = action.to ?? view.state.selection.main.to
                const value = action.insertValue ?? option.label
                if (action.asSnippet) {
                    const apply = snippet(value)
                    // {label: value} is just a dummy value to be able to use
                    // snippet(...)
                    apply(view, { label: value }, action.from, to)
                } else {
                    const changeSet = view.state.changeByRange(range => {
                        if (range === view.state.selection.main) {
                            return {
                                changes: {
                                    from: action.from,
                                    to,
                                    insert: value,
                                },
                                range: EditorSelection.cursor(action.from + value.length),
                            }
                        }
                        return { range }
                    })
                    view.dispatch({
                        ...changeSet,
                        effects: changeSet.effects.concat(setModeEffect.of(null)),
                        scrollIntoView: true,
                    })
                }
                notifySelectionListeners(view.state, option, action, source)
            }
            break
        case 'command':
            notifySelectionListeners(view.state, option, action, source)
            action.apply(option, view)
            break
        case 'goto':
            {
                const historyOrNavigate = view.state.facet(suggestionsConfig).historyOrNavigate
                if (historyOrNavigate) {
                    notifySelectionListeners(view.state, option, action, source)
                    compatNavigate(historyOrNavigate, action.url)
                    view.contentDOM.blur()
                }
            }
            break
    }
}

function notifySelectionListeners(state: EditorState, option: Option, action: Action, source: SelectionSource): void {
    const params = { option, action, source }
    for (const listener of state.facet(selectionListener)) {
        listener(params)
    }
}

const defaultKeyboardBindings: KeyBinding[] = [
    {
        key: 'ArrowDown',
        run: moveSelection('forward'),
    },
    {
        key: 'ArrowUp',
        run: moveSelection('backward'),
    },
    {
        mac: 'Ctrl-n',
        run: moveSelection('forward'),
    },
    {
        mac: 'Ctrl-p',
        run: moveSelection('backward'),
    },
    {
        key: 'Mod-Space',
        run(view) {
            startCompletion(view)
            return true
        },
    },
    {
        key: 'Enter',
        run(view) {
            const state = view.state.field(suggestionsStateField)
            const option = state.result.at(state.selectedOption)
            if (!state.open || !option) {
                return false
            }
            applyAction(view, option.action, option, 'keyboard')
            return true
        },
    },
    {
        key: 'Mod-Enter',
        run(view) {
            const state = view.state.field(suggestionsStateField)
            const option = state.result.at(state.selectedOption)
            if (state.open && option && option.alternativeAction) {
                applyAction(view, option.alternativeAction, option, 'keyboard')
            }
            return true
        },
    },
    {
        key: 'Escape',
        run(view) {
            if (view.state.field(suggestionsStateField).open) {
                view.dispatch({ effects: hideCompletionEffect.of() })
                return true
            }
            return false
        },
    },
]

export const suggestionSources = Facet.define<Source>({
    enables: [
        completionPlugin,
        suggestionsStateField,
        EditorView.updateListener.of(update => {
            if (
                update.focusChanged &&
                update.view.hasFocus &&
                update.view.state.field(suggestionsStateField).result.empty()
            ) {
                startCompletion(update.view)
            }
        }),
        Prec.highest(keymap.of(defaultKeyboardBindings)),
    ],
})

type SelectionSource = 'keyboard' | 'mouse'
export const selectionListener =
    Facet.define<(params: { option: Option; action: Action; source: SelectionSource }) => void>()

interface ExternalConfig extends Config {
    parent: HTMLDivElement
    source: Source
}

export const suggestions = ({ id, parent, source, historyOrNavigate }: ExternalConfig): Extension => [
    modesFacet.of([]), // makes sure the facet is defined
    suggestionsConfig.of({ historyOrNavigate, id }),
    suggestionSources.of(source),
    ViewPlugin.define(view => new SuggestionView(id, view, parent)),
]

/**
 * Load and show suggestions.
 */
export function startCompletion(view: EditorView): void {
    view.dispatch({ effects: startCompletionEffect.of() })
}
