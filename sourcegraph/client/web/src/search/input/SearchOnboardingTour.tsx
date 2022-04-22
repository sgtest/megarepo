/**
 * This file contains utility functions for the search onboarding tour.
 */
import { useCallback, useEffect, useMemo, useState } from 'react'

import classNames from 'classnames'
import * as H from 'history'
import { isEqual } from 'lodash'
import Shepherd from 'shepherd.js'
import Tour from 'shepherd.js/src/types/tour'

import { isMacPlatform } from '@sourcegraph/common'
import { QueryState } from '@sourcegraph/search'
import { MonacoQueryInputProps } from '@sourcegraph/search-ui'
import { ALL_LANGUAGES } from '@sourcegraph/shared/src/search/query/languageFilter'
import { scanSearchQuery } from '@sourcegraph/shared/src/search/query/scanner'
import { Token } from '@sourcegraph/shared/src/search/query/token'
import { useTemporarySetting } from '@sourcegraph/shared/src/settings/temporary/useTemporarySetting'
import { Button } from '@sourcegraph/wildcard'

import { eventLogger } from '../../tracking/eventLogger'
import { renderBrandedToString } from '../render-branded-to-string'

import { defaultPopperModifiers, defaultTourOptions } from './tour-options'

import styles from './SearchOnboardingTour.module.scss'

const tourOptions: Shepherd.Tour.TourOptions = {
    ...defaultTourOptions,
    defaultStepOptions: {
        ...defaultTourOptions.defaultStepOptions,
        popperOptions: {
            modifiers: [...defaultPopperModifiers, { name: 'offset', options: { offset: [0, 8] } }],
        },
    },
}

/**
 * generateStep creates the content for the search tour card. All steps that just contain
 * static content should use this function to populate the step's `text` field.
 */
function generateStep(options: { tour: Shepherd.Tour; stepNumber: number; content: HTMLElement }): HTMLElement {
    const element = document.createElement('div')
    element.className = `d-flex align-items-center test-tour-step-${options.stepNumber}`
    element.append(options.content)

    const close = document.createElement('div')
    close.className = 'd-flex align-items-center'
    close.innerHTML = `
         <div class=${styles.separator}></div>
         <div class=${styles.close}>${closeIconSvg}</div>
     `
    element.append(close)
    element.querySelector(`.${styles.close}`)?.addEventListener('click', () => {
        options.tour.cancel()
        eventLogger.log('CloseOnboardingTourClicked', { stage: options.stepNumber }, { stage: options.stepNumber })
    })

    return element
}

const closeIconSvg =
    '<svg width="16" height="16" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="M12.667 4.274l-.94-.94L8 7.06 4.273 3.334l-.94.94L7.06 8l-3.727 3.727.94.94L8 8.94l3.727 3.727.94-.94L8.94 8l3.727-3.726z" fill="currentColor"/></svg>'

/**
 * Generates the content for the first step in the tour.
 *
 * @param tour the Shepherd tour to attach the step to
 * @param languageButtonHandler the handler for the "search a language" button.
 * @param repositoryButtonHandler the handler for the "search a repository" button.
 */
function generateStep1(
    tour: Shepherd.Tour,
    languageButtonHandler: () => void,
    repositoryButtonHandler: () => void
): HTMLElement {
    const content = document.createElement('div')
    content.className = 'd-flex align-items-center'
    content.innerHTML = renderBrandedToString(
        <>
            <div className={styles.title}>Get started</div>
            <Button
                data-testid="tour-language-button"
                variant="link"
                className={classNames(styles.link, 'p-0 tour-language-button')}
            >
                Search a language
            </Button>
            <Button
                data-testid="tour-repo-button"
                variant="link"
                className={classNames(styles.link, 'p-0 tour-repo-button')}
            >
                Search a repository
            </Button>
        </>
    )
    content.querySelector('.tour-language-button')?.addEventListener('click', () => {
        languageButtonHandler()
        eventLogger.log('OnboardingTourLanguageOptionClicked')
    })
    content.querySelector('.tour-repo-button')?.addEventListener('click', () => {
        repositoryButtonHandler()
        eventLogger.log('OnboardingTourRepositoryOptionClicked')
    })

    return generateStep({ tour, content, stepNumber: 1 })
}

type TourStepID = 'filter-repository' | 'filter-lang' | 'add-query-term'

const TOUR_STEPS = ['filter-repository', 'filter-lang', 'add-query-term'] as TourStepID[]

/**
 * Returns `true` if, while on the filter-(repository|lang) step,
 * the search query is a (repo|lang) filter with no value.
 */
const shouldTriggerSuggestions = (currentTourStep: TourStepID | undefined, queryTokens: Token[]): boolean => {
    if (queryTokens.length !== 1) {
        return false
    }
    const filterToken = queryTokens[0]
    if (filterToken.type !== 'filter' || filterToken.value !== undefined) {
        return false
    }
    return currentTourStep === 'filter-repository'
        ? filterToken.field.value === 'repo'
        : currentTourStep === 'filter-lang'
        ? filterToken.field.value === 'lang'
        : false
}

/**
 * Returns `true` if, while on the filter-(repository|lang) step,
 * the search query is a valid (repo|lang) filter followed by whitespace.
 * -
 */
const shouldAdvanceLangOrRepoStep = (currentTourStep: TourStepID | undefined, queryTokens: Token[]): boolean => {
    if (queryTokens.length !== 2) {
        return false
    }
    const [filterToken, whitespaceToken] = queryTokens
    if (filterToken.type !== 'filter' || whitespaceToken.type !== 'whitespace') {
        return false
    }
    if (currentTourStep === 'filter-repository') {
        return filterToken.field.value === 'repo' && filterToken.value !== undefined
    }
    if (currentTourStep === 'filter-lang') {
        return (
            filterToken.field.value === 'lang' &&
            filterToken.value?.type === 'literal' &&
            ALL_LANGUAGES.includes(filterToken.value?.value)
        )
    }
    return false
}

/**
 * Returns true if, while on the add-query-term step, the search query
 * contains a search pattern.
 */
const shouldShowSubmitSearch = (currentTourStep: TourStepID | undefined, queryTokens: Token[]): boolean =>
    currentTourStep === 'add-query-term' && queryTokens.some(({ type }) => type === 'pattern')

/**
 * A hook returning the current step ID of the Shepherd Tour.
 */
const useCurrentStep = (tour: Tour): TourStepID | undefined => {
    const [currentStep, setCurrentStep] = useState<TourStepID | undefined>()
    useEffect(() => {
        setCurrentStep(TOUR_STEPS.find(stepID => isEqual(tour.getCurrentStep(), tour.getById(stepID))))
        const listener = ({ step }: { step: Shepherd.Step }): void => {
            setCurrentStep(TOUR_STEPS.find(stepID => isEqual(step, tour.getById(stepID))))
        }
        tour.on('show', listener)
        return () => {
            tour.off('show', listener)
        }
    }, [tour, setCurrentStep])
    return currentStep
}

const generateStepContent = (title: string, description: string): HTMLElement => {
    const element = document.createElement('div')
    element.className = 'd-flex align-items-center'
    element.innerHTML = `
         <div class=${styles.title}>${title}</div>
         <div class="${styles.description} text-monospace">${description}</div>
     `
    return element
}

const useTourWithSteps = ({
    setQueryState,
    stepsContainer,
}: Pick<UseSearchOnboardingTourOptions, 'setQueryState' | 'stepsContainer'>): Tour => {
    const tour = useMemo(() => new Shepherd.Tour({ ...tourOptions, stepsContainer }), [stepsContainer])
    useEffect(() => {
        tour.addSteps([
            {
                id: 'start-tour',
                text: generateStep1(
                    tour,
                    () => {
                        setQueryState({ query: 'lang:' })
                        tour.show('filter-lang')
                    },
                    () => {
                        setQueryState({ query: 'repo:' })
                        tour.show('filter-repository')
                    }
                ),
                // "tour-card" class usages are kept for testing in `search-onboarding.test.ts` since we can't add attributes into step elements
                classes: classNames(styles.tourCard, styles.tourCardArrowLeftUp, 'tour-card'),
                attachTo: {
                    element: '[data-search-page-input-container]',
                    on: 'bottom',
                },
                popperOptions: {
                    modifiers: [{ name: 'offset', options: { offset: [100, 8] } }],
                },
            },
            {
                id: 'filter-lang',
                text: generateStep({
                    tour,
                    stepNumber: 2,
                    content: generateStepContent('Enter a language', 'Example: Python'),
                }),
                when: {
                    show() {
                        eventLogger.log('ViewedOnboardingTourFilterLangStep')
                    },
                },
                classes: classNames(styles.tourCard, styles.tourCardArrowLeftDown, 'tour-card'),
                attachTo: {
                    element: '[data-search-page-input-container]',
                    on: 'top',
                },
                popperOptions: {
                    modifiers: [{ name: 'offset', options: { offset: [100, 8] } }],
                },
            },
            {
                id: 'filter-repository',
                text: generateStep({
                    tour,
                    stepNumber: 2,
                    content: generateStepContent('Enter a repository', 'Example: sourcegraph/sourcegraph'),
                }),
                when: {
                    show() {
                        eventLogger.log('ViewedOnboardingTourFilterRepoStep')
                    },
                },
                classes: classNames(styles.tourCard, styles.tourCardArrowLeftDown, 'tour-card'),
                attachTo: {
                    element: '[data-search-page-input-container]',
                    on: 'top',
                },
                popperOptions: {
                    modifiers: [{ name: 'offset', options: { offset: [100, 8] } }],
                },
            },
            {
                id: 'add-query-term',
                text: generateStep({
                    tour,
                    stepNumber: 3,
                    content: generateStepContent('Enter source code', 'Example: []*Request'),
                }),
                when: {
                    show() {
                        eventLogger.log('ViewedOnboardingTourAddQueryTermStep')
                    },
                },
                classes: classNames(styles.tourCard, styles.tourCardArrowLeftUp, 'tour-card'),
                attachTo: {
                    element: '[data-search-page-input-container]',
                    on: 'bottom',
                },
                popperOptions: {
                    modifiers: [{ name: 'offset', options: { offset: [100, 8] } }],
                },
            },
            {
                id: 'submit-search',
                text: generateStep({
                    tour,
                    stepNumber: 4,
                    content: generateStepContent('Search', `(Or press ${isMacPlatform() ? 'RETURN' : 'ENTER'})`),
                }),
                when: {
                    show() {
                        eventLogger.log('ViewedOnboardingTourSubmitSearchStep')
                    },
                },
                classes: classNames(styles.tourCard, styles.tourCardArrowRightDown, 'tour-card'),
                attachTo: {
                    element: '[data-search-button]',
                    on: 'top',
                },
                popperOptions: {
                    modifiers: [{ name: 'offset', options: { offset: [-140, 8] } }],
                },
                advanceOn: { selector: '[data-search-button]', event: 'click' },
            },
        ])
    }, [tour, setQueryState])
    return tour
}

interface UseSearchOnboardingTourOptions {
    /**
     * Whether the onboarding tour feature flag is enabled.
     */
    showOnboardingTour: boolean

    /**
     * A callback allowing the onboarding tour to trigger
     * updates to the search query.
     */
    setQueryState: (queryState: QueryState) => void

    /**
     * The query currently displayed in the query input.
     */
    queryState: QueryState
    history: H.History
    location: H.Location

    /**
     * HTML element where the steps should be attached to
     */
    stepsContainer?: HTMLElement
}

/**
 * Represents the object returned by `useSearchOnboardingTour`.
 *
 * The subset of MonacoQueryInput props should be passed down to the input component.
 */
interface UseSearchOnboardingTourReturnValue
    extends Pick<MonacoQueryInputProps, 'onCompletionItemSelected' | 'onSuggestionsInitialized' | 'onFocus'> {
    /**
     * Whether the query input should be focused by default
     * (`false` on the search homepage when the tour is active).
     */
    shouldFocusQueryInput: boolean
}

/**
 * A hook that handles displaying and running the search onboarding tour,
 * to be used in conjunction with the MonacoQueryInput.
 *
 * See {@link UseSearchOnboardingTourOptions} and {@link UseSearchOnboardingTourReturnValue}
 */
export const useSearchOnboardingTour = ({
    showOnboardingTour,
    queryState,
    setQueryState,
    stepsContainer,
}: UseSearchOnboardingTourOptions): UseSearchOnboardingTourReturnValue => {
    const tour = useTourWithSteps({ setQueryState, stepsContainer })
    // True when the user has manually cancelled the tour
    const [hasCancelledTour, setHasCancelledTour] = useTemporarySetting('search.onboarding.tourCancelled', false)
    const [daysActiveCount] = useTemporarySetting('user.daysActiveCount', 0)

    const shouldShowTour = useMemo(() => showOnboardingTour && daysActiveCount === 1 && !hasCancelledTour, [
        showOnboardingTour,
        daysActiveCount,
        hasCancelledTour,
    ])

    // Start the Tour when the query input is focused on the search homepage.
    const onFocus = useCallback(() => {
        if (shouldShowTour && !tour.isActive()) {
            tour.start()
        }
    }, [shouldShowTour, tour])

    // Hook into Tour cancellation event.
    useEffect(() => {
        const onCancelled = (): void => {
            setHasCancelledTour(true)
        }
        tour.on('cancel', onCancelled)
        return () => {
            tour.off('cancel', onCancelled)
        }
    }, [tour, setHasCancelledTour])

    // 'Complete' tour on unmount.
    useEffect(
        () => () => {
            // Shepherd does not mix well with multiple active tours (because it stores the current active tour in a mutable global variable).
            // This causes weird behaviour where cancelling one tour cancells other tours as well. Cancelling the tour doesn't always remove it
            // from the DOM, so we have to manually hide it (e.g. when navigating between pages).
            tour.hide()
            if (tour.isActive()) {
                tour.complete()
            }
        },
        [tour]
    )

    useEffect(() => {
        if (shouldShowTour) {
            eventLogger.log('ViewOnboardingTour')
        }
    }, [tour, shouldShowTour])

    // A handle allowing to trigger display of the MonacoQueryInput suggestions widget.
    const [suggestions, onSuggestionsInitialized] = useState<{ trigger: () => void }>()

    // On query or step changes, advance the Tour if appropriate.
    const currentStep = useCurrentStep(tour)
    const queryTokens = useMemo((): Token[] => {
        const scannedQuery = scanSearchQuery(queryState.query)
        return scannedQuery.type === 'success' ? scannedQuery.term : []
    }, [queryState.query])
    useEffect(() => {
        if (!tour.isActive()) {
            return
        }
        if (shouldTriggerSuggestions(currentStep, queryTokens)) {
            suggestions?.trigger()
        } else if (shouldAdvanceLangOrRepoStep(currentStep, queryTokens)) {
            tour.show('add-query-term')
        } else if (shouldShowSubmitSearch(currentStep, queryTokens)) {
            tour.show('submit-search')
        }
    }, [suggestions, queryTokens, tour, currentStep])

    // When a completion item is selected,
    // advance the repo or lang step if appropriate.
    const onCompletionItemSelected = useCallback(() => {
        if (shouldAdvanceLangOrRepoStep(currentStep, queryTokens)) {
            tour.show('add-query-term')
        }
    }, [queryTokens, tour, currentStep])

    return {
        onCompletionItemSelected,
        onFocus,
        onSuggestionsInitialized,
        shouldFocusQueryInput: !shouldShowTour,
    }
}
