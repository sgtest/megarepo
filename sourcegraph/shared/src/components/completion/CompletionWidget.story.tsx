import { action } from '@storybook/addon-actions'
import { text } from '@storybook/addon-knobs'
import { storiesOf } from '@storybook/react'
import React, { useState } from 'react'
import { CompletionList } from 'sourcegraph'
import { CompletionWidget, CompletionWidgetProps } from './CompletionWidget'
import bootstrapStyles from 'bootstrap/scss/bootstrap.scss'
import completionWidgetStyles from './CompletionWidget.scss'

const onSelectItem = action('onSelectItem')

// Disable keyboard shortcuts because in CompletionWidget the cursor is in a contenteditable element,
// which Storybook doesn't consider to be an input, so it intercepts keyboard shortcuts instead of
// propagating them to the CompletionWidget element.
const { add } = storiesOf('shared/CompletionWidget', module)
    .addParameters({ options: { enableShortcuts: false } })
    .addDecorator(story => (
        <>
            <style>{bootstrapStyles}</style>
            <style>{completionWidgetStyles}</style>
            <div>{story()}</div>
        </>
    ))

const completionWidgetListItemClassName = 'completion-widget-dropdown__item d-flex align-items-center p-2'
const StyledCompletionWidget: React.FunctionComponent<CompletionWidgetProps> = props => (
    <CompletionWidget
        {...props}
        listClassName="completion-widget-dropdown d-block list-unstyled rounded border p-0 m-0 mt-3"
        listItemClassName={completionWidgetListItemClassName}
        selectedListItemClassName="completion-widget-dropdown__item--selected bg-primary"
        loadingClassName={completionWidgetListItemClassName}
        noResultsClassName={completionWidgetListItemClassName}
    />
)
StyledCompletionWidget.displayName = 'StyledCompletionWidget'

add('interactive', () => {
    const CompletionWidgetInteractive: React.FunctionComponent = () => {
        const [element, setElement] = useState<HTMLTextAreaElement | null>(null)
        return (
            <div className="position-relative p-5">
                {element && (
                    <StyledCompletionWidget
                        completionListOrError={
                            { items: [{ label: 'alice' }, { label: 'bob' }, { label: 'carol' }] } as CompletionList
                        }
                        textArea={element}
                        onSelectItem={onSelectItem}
                        listClassName="list-unstyled"
                    />
                )}
                <textarea
                    ref={setElement}
                    rows={8}
                    className="w-100"
                    defaultValue={text('Initial value', 'hello, world!')}
                />
            </div>
        )
    }
    return <CompletionWidgetInteractive />
})
