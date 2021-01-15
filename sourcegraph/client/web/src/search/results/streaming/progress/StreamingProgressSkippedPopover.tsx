import classNames from 'classnames'
import * as H from 'history'
import AlertCircleIcon from 'mdi-react/AlertCircleIcon'
import ChevronDownIcon from 'mdi-react/ChevronDownIcon'
import ChevronLeftIcon from 'mdi-react/ChevronLeftIcon'
import InformationOutlineIcon from 'mdi-react/InformationOutlineIcon'
import SearchIcon from 'mdi-react/SearchIcon'
import React, { useCallback, useState } from 'react'
import { Button, Collapse, Form, FormGroup, Input, Label } from 'reactstrap'
import { Markdown } from '../../../../../../shared/src/components/Markdown'
import { renderMarkdown } from '../../../../../../shared/src/util/markdown'
import { SyntaxHighlightedSearchQuery } from '../../../../components/SyntaxHighlightedSearchQuery'
import { Skipped } from '../../../stream'
import { StreamingProgressProps } from './StreamingProgress'

const severityToNumber = (severity: Skipped['severity']): number => {
    switch (severity) {
        case 'error':
            return 1
        case 'warn':
            return 2
        case 'info':
            return 3
    }
}

const sortBySeverity = (a: Skipped, b: Skipped): number => {
    const aSev = severityToNumber(a.severity)
    const bSev = severityToNumber(b.severity)

    return aSev - bSev
}

const SkippedMessage: React.FunctionComponent<{ skipped: Skipped; history: H.History; startOpen: boolean }> = ({
    skipped,
    history,
    startOpen,
}) => {
    const [isOpen, setIsOpen] = useState(startOpen)

    const toggleIsOpen = useCallback(() => setIsOpen(oldValue => !oldValue), [])

    return (
        <Button
            className={classNames('streaming-skipped-item pt-3 pb-0 w-100', {
                'streaming-skipped-item--warn': skipped.severity !== 'info',
            })}
            onClick={toggleIsOpen}
            disabled={!skipped.message}
        >
            <h4 className="d-flex align-items-center mb-0 w-100">
                {skipped.severity === 'info' ? (
                    <InformationOutlineIcon className="icon-inline mr-2 streaming-skipped-item__icon flex-shrink-0" />
                ) : (
                    <AlertCircleIcon className="icon-inline mr-2 streaming-skipped-item__icon flex-shrink-0" />
                )}
                <span className="flex-grow-1 text-left">{skipped.title}</span>

                {skipped.message &&
                    (isOpen ? (
                        <ChevronDownIcon className="icon-inline flex-shrink-0" />
                    ) : (
                        <ChevronLeftIcon className="icon-inline flex-shrink-0" />
                    ))}
            </h4>
            {skipped.message && (
                <Collapse isOpen={isOpen}>
                    <Markdown
                        className="streaming-skipped-item__message mt-2 text-left"
                        dangerousInnerHTML={renderMarkdown(skipped.message)}
                        history={history}
                    />
                </Collapse>
            )}
            <div className="streaming-skipped-item__bottom-border-spacer mt-3" />
        </Button>
    )
}

export const StreamingProgressSkippedPopover: React.FunctionComponent<
    Pick<StreamingProgressProps, 'progress' | 'onSearchAgain' | 'history'>
> = ({ progress, onSearchAgain, history }) => {
    const [selectedSuggestedSearches, setSelectedSuggestedSearches] = useState(new Set<string>())
    const submitHandler = useCallback(
        (event: React.FormEvent) => {
            onSearchAgain([...selectedSuggestedSearches])
            event.preventDefault()
        },
        [selectedSuggestedSearches, onSearchAgain]
    )
    const checkboxHandler = useCallback((event: React.FormEvent<HTMLInputElement>) => {
        const itemToToggle = event.currentTarget.value
        const checked = event.currentTarget.checked
        setSelectedSuggestedSearches(selected => {
            const newSelected = new Set(selected)
            if (checked) {
                newSelected.add(itemToToggle)
            } else {
                newSelected.delete(itemToToggle)
            }
            return newSelected
        })
    }, [])

    const sortedSkippedItems = progress.skipped.sort(sortBySeverity)

    return (
        <>
            {sortedSkippedItems.map((skipped, index) => (
                <SkippedMessage
                    key={skipped.reason}
                    skipped={skipped}
                    history={history}
                    startOpen={index === 0 && skipped.severity !== 'info'} // Start with first item open, but only if it's not info severity
                />
            ))}
            {sortedSkippedItems.some(skipped => skipped.suggested) && (
                <Form className="pb-3 px-3" onSubmit={submitHandler}>
                    <div className="mb-2 mt-3">Search again:</div>
                    <FormGroup check={true}>
                        {sortedSkippedItems.map(
                            skipped =>
                                skipped.suggested && (
                                    <Label
                                        check={true}
                                        className="mb-1 d-block"
                                        key={skipped.suggested.queryExpression}
                                    >
                                        <Input
                                            type="checkbox"
                                            value={skipped.suggested.queryExpression}
                                            onChange={checkboxHandler}
                                        />{' '}
                                        {skipped.suggested.title} (
                                        <SyntaxHighlightedSearchQuery query={skipped.suggested.queryExpression} />)
                                    </Label>
                                )
                        )}
                    </FormGroup>

                    <Button
                        type="submit"
                        className="mt-2"
                        color="primary"
                        disabled={selectedSuggestedSearches.size === 0}
                    >
                        <SearchIcon className="icon-inline mr-1" />
                        Search again
                    </Button>
                </Form>
            )}
        </>
    )
}
