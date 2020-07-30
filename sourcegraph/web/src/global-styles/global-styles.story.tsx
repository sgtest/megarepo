// This story is NOT a complete replication of the Bootstrap documentation. This means it is not an exhaustive
// documentation of all the Bootstrap classes we have available in our app, please see refer to the Bootstrap
// documentation for that. Its primary purpose is to show what Bootstrap's componenents look like with our styling
// customizations.

import React from 'react'
import { storiesOf } from '@storybook/react'
import classNames from 'classnames'
import webStyles from '../SourcegraphWebApp.scss'
import { action } from '@storybook/addon-actions'
import { radios } from '@storybook/addon-knobs'
import { flow, startCase } from 'lodash'
import { highlightCodeSafe } from '../../../shared/src/util/markdown'
import { Form } from '../components/Form'
import openColor from 'open-color'
import { Menu, MenuButton, MenuList, MenuLink } from '@reach/menu-button'
import 'storybook-addon-designs'

const semanticColors = ['primary', 'secondary', 'success', 'danger', 'warning', 'info', 'merged'] as const

const preventDefault = <E extends React.SyntheticEvent>(event: E): E => {
    event.preventDefault()
    return event
}

const { add } = storiesOf('web/Global styles', module).addDecorator(story => {
    // TODO find a way to do this globally for all stories and storybook itself.
    const theme = radios('Theme', { Light: 'light', Dark: 'dark' }, 'light')
    document.body.classList.toggle('theme-light', theme === 'light')
    document.body.classList.toggle('theme-dark', theme === 'dark')
    return (
        <>
            <div className="p-3 container">{story()}</div>
            <style>{webStyles}</style>
        </>
    )
})

add('Text', () => (
    <>
        <h1>Typography</h1>
        <h2>Prose</h2>
        <p>Text uses system fonts. The fonts should never be overridden.</p>
        <p>
            Minim nisi tempor Lorem do incididunt exercitation ipsum consectetur laboris elit est aute irure velit.
            Voluptate irure excepteur sint reprehenderit culpa laboris. Elit id nostrud enim laboris irure. Est sunt ex
            adipisicing aute elit voluptate consectetur. Do laboris anim fugiat ipsum sunt elit sunt amet consequat
            nostrud irure labore cupidatat laboris. Voluptate eiusmod veniam nisi reprehenderit cillum Lorem veniam
            fugiat amet ea dolore enim. Ea laborum fugiat Lorem ea amet amet exercitation dolor culpa. Do consequat
            labore dolor ad elit ipsum nostrud non laboris voluptate aliquip est reprehenderit incididunt. Eu nulla ad
            voluptate enim. Pariatur duis pariatur sit adipisicing pariatur nulla quis do sint deserunt aliqua Lorem
            tempor laborum. Dolor esse aute cupidatat deserunt anim ad eiusmod quis quis laborum magna nisi occaecat. Eu
            aliquip duis eiusmod sint aliquip duis est sit irure velit reprehenderit id. Cillum est esse et nulla ut
            adipisicing velit anim id exercitation nostrud. Duis veniam sit laboris tempor quis sit cupidatat elit.
        </p>

        <p>
            Text can be <em>emphasized</em> or made <strong>strong</strong>.
        </p>

        <p>
            Text can have superscripts<sup>sup</sup> with <code>{'<sup>'}</code>.
        </p>

        <p>
            Text can have subscripts<sub>sub</sub> with <code>{'<sub>'}</code>.
        </p>

        <p>
            <small>
                You can use <code>{'<small>'}</code> to make small text. Use sparingly.
            </small>
        </p>

        <h2>Headings</h2>
        <table className="table">
            <tbody>
                {(['h1', 'h2', 'h3', 'h4', 'h5', 'h6'] as const).map(Heading => (
                    <tr key={Heading}>
                        <td>
                            <code>
                                {'<'}
                                {Heading}
                                {'>'}
                            </code>
                        </td>
                        <td>
                            <Heading>Heading</Heading>
                        </td>
                    </tr>
                ))}
            </tbody>
        </table>

        <h2>Lists</h2>
        <ol>
            <li>Item</li>
            <li>Item</li>
            <li>Item</li>
        </ol>
        <ul>
            <li>Item</li>
            <li>Item</li>
            <li>Item</li>
        </ul>
    </>
))

add('Code', () => (
    <>
        <h1>Code</h1>

        <h2>Inline Code</h2>
        <p>
            Example of <code>inline code</code> that can be achieved with the <code>{'<code>'}</code> element.
        </p>

        <h2>Highlighted multi-line code</h2>
        <p>Custom highlight.js themes are defined for both light and dark themes.</p>

        <h3>TypeScript</h3>
        <pre>
            <code
                dangerouslySetInnerHTML={{
                    __html: highlightCodeSafe(
                        ['const foo = 123', 'const bar = "Hello World!"', 'console.log(foo)'].join('\n'),
                        'typescript'
                    ),
                }}
            />
        </pre>

        <h3>JSON</h3>
        <pre>
            <code
                dangerouslySetInnerHTML={{
                    __html: highlightCodeSafe(
                        ['{', '  "someString": "Hello World!",', '  "someNumber": 123', '}'].join('\n'),
                        'json'
                    ),
                }}
            />
        </pre>

        <h3>Diffs</h3>
        <pre>
            <code
                dangerouslySetInnerHTML={{
                    __html: highlightCodeSafe(
                        [
                            ' const foo = 123',
                            '-const bar = "Hello, world!"',
                            '+const bar = "Hello, traveller!"',
                            ' console.log(foo)',
                        ].join('\n'),
                        'diff'
                    ),
                }}
            />
        </pre>

        <h2>Keyboard shortcuts</h2>
        <p>
            Keyboard shortcuts should use <code>{'<kbd>'}</code>, not <code>{'<code>'}</code>. For example,{' '}
            <kbd>cmd</kbd>+<kbd>C</kbd> is used to copy text to the clipboard.
        </p>
    </>
))

add(
    'Colors',
    () => (
        <>
            <h1>Colors</h1>

            <h2>Semantic colors</h2>
            <p>
                These can be used to give semantic clues and always work both in light and dark theme. They are
                available on most CSS components and the <code>border-</code> and <code>bg-</code> utility classes.
            </p>
            <div className="d-flex flex-wrap">
                {semanticColors.map(semantic => (
                    <div className="m-2 text-center" key={semantic}>
                        <div className={`bg-${semantic} rounded`} style={{ width: '5rem', height: '5rem' }} />
                        {semantic}
                    </div>
                ))}
            </div>

            <h2>Color Palette</h2>
            <p>
                Our color palette is the <a href="https://yeun.github.io/open-color/">Open Color</a> palette. All colors
                are available as SCSS and CSS variables. It's generally not advised to use these directly, but they may
                be used in rare cases, like charts. In other cases, rely on CSS components, utilities for borders and
                background, and dynamic CSS variables.
            </p>
            {Object.entries(openColor).map(
                ([name, colors]) =>
                    Array.isArray(colors) && (
                        <div key={name}>
                            <h5>{name}</h5>
                            <div className="d-flex flex-wrap">
                                {colors.map((color, number) => (
                                    <div key={color} className="m-2 text-right">
                                        <div
                                            className="rounded"
                                            style={{ background: color, width: '3rem', height: '3rem' }}
                                        />
                                        {number}
                                    </div>
                                ))}
                            </div>
                        </div>
                    )
            )}
        </>
    ),
    {
        design: {
            type: 'figma',
            url: 'https://www.figma.com/file/P2M4QrgIxeUsjE80MHP8TmY3/Sourcegraph-Colors?node-id=0%3A2',
        },
    }
)

add('Layout', () => (
    <>
        <h1>Layout</h1>

        <h2>Spacing</h2>
        <p>
            Use margin <code>m-*</code> and padding <code>p-*</code> utilities to align with the{' '}
            <a
                href="https://builttoadapt.io/intro-to-the-8-point-grid-system-d2573cde8632"
                target="_blank"
                rel="noopener noreferrer"
            >
                8pt grid
            </a>
            . When hand-writing CSS, use <code>rem</code> units in multiples of <code>0.25</code>.
        </p>

        <h2>One-dimensional layout</h2>
        <p>
            Use{' '}
            <a href="https://css-tricks.com/snippets/css/a-guide-to-flexbox/" target="_blank" rel="noopener noreferrer">
                Flexbox
            </a>{' '}
            for one-dimensional layouts (single rows or columns, with optional wrapping). You can use{' '}
            <a href="https://getbootstrap.com/docs/4.5/utilities/flex/" target="_blank" rel="noopener noreferrer">
                utility classes
            </a>{' '}
            for simple flexbox layouts.
        </p>

        <h3>Row layout</h3>
        <h4>Equally distributed</h4>
        <div
            className="d-flex p-1 border mb-2 overflow-hidden"
            style={{ resize: 'both', minWidth: '16rem', minHeight: '3rem' }}
        >
            <div className="p-1 m-1 flex-grow-1 d-flex justify-content-center align-items-center border">Column 1</div>
            <div className="p-1 m-1 flex-grow-1 d-flex justify-content-center align-items-center border">Column 2</div>
            <div className="p-1 m-1 flex-grow-1 d-flex justify-content-center align-items-center border">Column 3</div>
        </div>

        <h4>Middle column growing</h4>
        <div
            className="d-flex p-1 border mb-2 overflow-hidden"
            style={{ resize: 'both', minWidth: '16rem', minHeight: '3rem' }}
        >
            <div className="p-1 m-1 d-flex justify-content-center align-items-center border border">Column 1</div>
            <div className="p-1 m-1 d-flex justify-content-center align-items-center border flex-grow-1 border">
                Column 2
            </div>
            <div className="p-1 m-1 d-flex justify-content-center align-items-center border border">Column 3</div>
        </div>

        <h3>Column layout</h3>
        <div
            className="d-flex flex-column p-1 border mb-2 overflow-hidden"
            style={{ minHeight: '8rem', height: '12rem', minWidth: '6rem', width: '12rem', resize: 'both' }}
        >
            <div className="p-1 m-1 flex-grow-1 border d-flex align-items-center justify-content-center">Row 1</div>
            <div className="p-1 m-1 flex-grow-1 border d-flex align-items-center justify-content-center">Row 2</div>
            <div className="p-1 m-1 flex-grow-1 border d-flex align-items-center justify-content-center">Row 3</div>
        </div>

        <h2>Two-dimensional layout</h2>
        <p>
            Use <a href="https://learncssgrid.com/">CSS Grid</a> for complex two-dimensional layouts.
        </p>
        <div
            className="p-2 border overflow-hidden"
            style={{
                display: 'grid',
                gridTemplateColumns: 'repeat(3, 1fr)',
                gridAutoRows: '1fr',
                gridGap: '0.5rem',
                resize: 'both',
                minWidth: '16rem',
                height: '16rem',
                minHeight: '6rem',
                marginBottom: '16rem',
            }}
        >
            <div className="border d-flex align-items-center justify-content-center">Cell 1</div>
            <div className="border d-flex align-items-center justify-content-center">Cell 2</div>
            <div className="border d-flex align-items-center justify-content-center">Cell 3</div>
            <div className="border d-flex align-items-center justify-content-center">Cell 4</div>
            <div className="border d-flex align-items-center justify-content-center">Cell 5</div>
            <div className="border d-flex align-items-center justify-content-center">Cell 6</div>
        </div>
    </>
))

add(
    'Alerts',
    () => (
        <>
            <h1>Alerts</h1>
            <p>
                Provide contextual feedback messages for typical user actions with the handful of available and flexible
                alert messages.
            </p>
            {semanticColors.map(semantic => (
                <div key={semantic} className={classNames('alert', `alert-${semantic}`)}>
                    A simple {semantic} alert — check it out! It can also contain{' '}
                    <a href="" className="alert-link" onClick={flow(preventDefault, action('alert link clicked'))}>
                        links like this
                    </a>
                    .
                </div>
            ))}
        </>
    ),
    {
        design: {
            type: 'figma',
            url: 'https://www.figma.com/file/BkY8Ak997QauG0Iu2EqArv/Sourcegraph-Components?node-id=127%3A4',
        },
    }
)

add(
    'Badges',
    () => (
        <>
            <h1>Badges</h1>
            <p>
                <a href="https://sourcegraph.com/search?q=repo:%5Egithub%5C.com/sourcegraph/sourcegraph%24+count:1000+badge+badge-&patternType=literal">
                    Usages
                </a>{' '}
                | <a href="https://getbootstrap.com/docs/4.5/components/badge/">Bootstrap Documentation</a>{' '}
            </p>
            <p>Badges are used for labelling and displaying small counts.</p>

            <h2>Scaling</h2>
            <p>
                Badges scale to match the size of the immediate parent element by using relative font sizing and{' '}
                <code>em</code> units for padding.
            </p>
            <p>
                Use a superscript <code>{'<sup></sup>'}</code> to position the badge top-right of a word in{' '}
                <code>h1</code> headings. Do not use a superscript for smaller text, because the font size would become
                too small.
            </p>
            <table className="table">
                <tbody>
                    <tr>
                        <td>
                            <code>{'<h1>'}</code> + <code>{'<sup>'}</code>
                        </td>
                        <td>
                            <h1>
                                Lorem{' '}
                                <sup>
                                    <span className="badge badge-secondary">ipsum</span>
                                </sup>
                            </h1>
                            <small>Use a superscript to align the badge top-right of the heading text.</small>
                        </td>
                    </tr>
                    {(['h2', 'h3', 'h4', 'h5', 'h6'] as const).map(Heading => (
                        <tr key={Heading}>
                            <td>
                                <code>{`<${Heading}>`}</code>
                            </td>
                            <td>
                                <Heading>
                                    Lorem <span className="badge badge-secondary">ipsum</span>
                                </Heading>
                            </td>
                        </tr>
                    ))}
                    <tr>
                        <td>Regular text</td>
                        <td>
                            Lorem <span className="badge badge-secondary">ipsum</span>
                        </td>
                    </tr>
                    <tr>
                        <td>
                            <code>{'<small>'}</code>
                        </td>
                        <td>
                            <small>
                                Lorem <span className="badge badge-secondary">ipsum</span>
                            </small>
                            <p>
                                <small className="text-danger">
                                    Discouraged because the text becomes too small to read.
                                </small>
                            </p>
                        </td>
                    </tr>
                </tbody>
            </table>

            <h2>Semantic variations</h2>
            <p>Change the appearance of any badge with modifier classes for semantic colors.</p>
            <p>
                {semanticColors.map(semantic => (
                    <React.Fragment key={semantic}>
                        <span className={classNames('badge', `badge-${semantic}`)}>{semantic}</span>{' '}
                    </React.Fragment>
                ))}
            </p>

            <h2>Uppercase</h2>
            <p>
                Badges can be visually uppercased by combining them with the <code>text-uppercase</code> class.
                Examples:
            </p>
            <div>
                <h1>
                    Blockchain support{' '}
                    <sup>
                        <span className="badge badge-warning text-uppercase">Beta</span>
                    </sup>
                </h1>
                <h1>
                    Blockchain support{' '}
                    <sup>
                        <span className="badge badge-info text-uppercase">Preview</span>
                    </sup>
                </h1>
                <h1>
                    Blockchain support{' '}
                    <sup>
                        <span className="badge badge-info text-uppercase">Experimental</span>
                    </sup>
                </h1>
                <h1>
                    Blockchain support{' '}
                    <sup>
                        <span className="badge badge-info text-uppercase">Prototype</span>
                    </sup>
                </h1>
            </div>
            <p>
                <span className="badge badge-success text-uppercase">added</span> <code>path/to/file.ts</code>
            </p>
            <p>
                <span className="badge badge-danger text-uppercase">deleted</span> <code>path/to/file.ts</code>
            </p>
            <p>
                <span className="badge badge-warning text-uppercase">moved</span> <code>path/to/file.ts</code>
            </p>
            <p>Do not use it for user-supplied text like labels (tags) or usernames.</p>

            <h2>Pill badges</h2>
            <p>Pill badges are commonly used to display counts.</p>
            <div className="mb-4">
                Matches <span className="badge badge-pill badge-secondary">321+</span>
            </div>
            <div>
                <ul className="nav nav-tabs">
                    <li className="nav-item">
                        <a className="nav-link active" href="#" onClick={preventDefault}>
                            Comments <span className="badge badge-pill badge-secondary">14</span>
                        </a>
                    </li>
                    <li className="nav-item">
                        <a className="nav-link" href="#" onClick={preventDefault}>
                            Changed files <span className="badge badge-pill badge-secondary">6</span>
                        </a>
                    </li>
                </ul>
            </div>
        </>
    ),
    {
        design: {
            type: 'figma',
            url: 'https://www.figma.com/file/BkY8Ak997QauG0Iu2EqArv/Sourcegraph-Components?node-id=486%3A0',
        },
    }
)

add(
    'Buttons',
    () => (
        <>
            <h1>Buttons</h1>
            <p>
                Use Bootstrap’s custom button styles for actions in forms, dialogs, and more with support for multiple
                sizes, states, and more.{' '}
                <a href="https://getbootstrap.com/docs/4.5/components/buttons/">Bootstrap documentation</a>
            </p>
            <p>
                {semanticColors.map(semantic => (
                    <React.Fragment key={semantic}>
                        <button
                            type="button"
                            key={semantic}
                            className={classNames('btn', `btn-${semantic}`)}
                            onClick={flow(preventDefault, action('button clicked'))}
                        >
                            {startCase(semantic)}
                        </button>{' '}
                    </React.Fragment>
                ))}
            </p>

            <h2>Disabled</h2>
            <p>
                <button type="button" className="btn btn-primary" disabled={true}>
                    I am disabled
                </button>{' '}
                <button type="button" className="btn btn-secondary" disabled={true}>
                    I am disabled
                </button>
            </p>

            <h2>Links</h2>
            <p>Links can be made to look like buttons too.</p>
            <a href="https://example.com" className="btn btn-secondary" target="_blank" rel="noopener noreferrer">
                I am a link
            </a>
        </>
    ),
    {
        design: {
            type: 'figma',
            url: 'https://www.figma.com/file/BkY8Ak997QauG0Iu2EqArv/Sourcegraph-Components?node-id=35%3A11',
        },
    }
)

add('Dropdowns', () => (
    <>
        <h1>Dropdowns</h1>
        <p>
            Toggle contextual overlays for displaying lists of links and more with the Bootstrap dropdown component.{' '}
            <a href="https://getbootstrap.com/docs/4.5/components/dropdowns/">Bootstrap documentation</a>
        </p>
        <Menu>
            <MenuButton className="btn btn-secondary dropdown-toggle">Dropdown button</MenuButton>
            <MenuList className="dropdown-menu show" style={{ outline: 'none' }}>
                <h6 className="dropdown-header">Dropdown header</h6>
                <MenuLink
                    className="dropdown-item"
                    href="https://example.com"
                    target="_blank"
                    rel="noopener noreferrer"
                >
                    Action
                </MenuLink>
                <MenuLink
                    className="dropdown-item"
                    href="https://example.com"
                    target="_blank"
                    rel="noopener noreferrer"
                >
                    Another action
                </MenuLink>
                <div className="dropdown-divider" />
                <MenuLink
                    className="dropdown-item"
                    href="https://example.com"
                    target="_blank"
                    rel="noopener noreferrer"
                >
                    Something else here
                </MenuLink>
            </MenuList>
        </Menu>
    </>
))

add('Input groups', () => (
    <>
        <h1>Input groups</h1>

        <p>
            Easily extend form controls by adding text, buttons, or button groups on either side of textual inputs,
            custom selects, and custom file inputs.{' '}
            <a href="https://getbootstrap.com/docs/4.5/components/input-group/">Bootstrap documentation</a>
        </p>

        <h2>Example</h2>
        <div>
            <div className="input-group" style={{ maxWidth: '24rem' }}>
                <input type="search" className="form-control" placeholder="Search code..." aria-label="Search query" />
                <div className="input-group-append">
                    <button className="btn btn-primary" type="submit">
                        Submit
                    </button>
                </div>
            </div>
        </div>
    </>
))

add(
    'Forms',
    () => (
        <>
            <h1>Forms</h1>
            <p>
                Forms are validated using native HTML validation. Submit the below form with invalid input to try it
                out.{' '}
                <a href="https://getbootstrap.com/docs/4.5/components/forms/" target="_blank" rel="noopener noreferrer">
                    Bootstrap documentation
                </a>
            </p>
            <Form onSubmit={preventDefault}>
                <div className="form-group">
                    <label htmlFor="example-email-input">Email address</label>
                    <input
                        type="email"
                        className="form-control"
                        id="example-email-input"
                        aria-describedby="email-help"
                        placeholder="me@example.com"
                    />
                    <small id="email-help" className="form-text text-muted">
                        We'll never share your email with anyone else.
                    </small>
                </div>
                <div className="form-group">
                    <label htmlFor="example-input-password">Password</label>
                    <input type="password" className="form-control" id="example-input-password" />
                </div>
                <div className="form-group">
                    <label htmlFor="example-example-select">Example select</label>
                    <select id="example-select" className="form-control">
                        <option>Option A</option>
                        <option>Option B</option>
                        <option>Option C</option>
                    </select>
                </div>
                <div className="form-group">
                    <label htmlFor="example-textarea">Example textarea</label>
                    <textarea className="form-control" id="example-textarea" rows={3} />
                </div>
                <div className="form-group form-check">
                    <input type="checkbox" className="form-check-input" id="exampleCheck1" />
                    <label className="form-check-label" htmlFor="exampleCheck1">
                        Check me out
                    </label>
                </div>
                <button type="submit" className="btn btn-primary">
                    Submit
                </button>
            </Form>

            <h2 className="mt-3">Disabled</h2>
            <Form>
                <fieldset disabled={true}>
                    <div className="form-group">
                        <label htmlFor="disabledTextInput">Disabled input</label>
                        <input
                            type="text"
                            id="disabledTextInput"
                            className="form-control"
                            placeholder="Disabled input"
                        />
                    </div>
                    <div className="form-group">
                        <label htmlFor="disabledSelect">Disabled select menu</label>
                        <select id="disabledSelect" className="form-control">
                            <option>Disabled select</option>
                        </select>
                    </div>
                    <div className="form-group">
                        <div className="form-check">
                            <input
                                className="form-check-input"
                                type="checkbox"
                                id="disabledFieldsetCheck"
                                disabled={true}
                            />
                            <label className="form-check-label" htmlFor="disabledFieldsetCheck">
                                Can't check this
                            </label>
                        </div>
                    </div>
                    <button type="submit" className="btn btn-primary">
                        Submit
                    </button>
                </fieldset>
            </Form>

            <h2 className="mt-3">Readonly</h2>
            <input className="form-control" type="text" value="I'm a readonly value" readOnly={true} />

            <h2 className="mt-3">Sizing</h2>
            <p>Form controls can be made smaller or larger for rare use cases, like a select inside a dropdown menu.</p>
            <div className="d-flex">
                <div>
                    <input className="form-control form-control-lg mb-1" type="text" placeholder="Large input" />
                    <input className="form-control mb-1" type="text" placeholder="Default input" />
                    <input className="form-control form-control-sm mb-1" type="text" placeholder="Small input" />
                </div>
                <div className="ml-2">
                    <select className="form-control form-control-lg mb-1">
                        <option>Large select</option>
                    </select>
                    <select className="form-control mb-1">
                        <option>Default select</option>
                    </select>
                    <select className="form-control form-control-sm mb-1">
                        <option>Small select</option>
                    </select>
                </div>
            </div>
        </>
    ),
    {
        design: {
            type: 'figma',
            url: 'https://www.figma.com/file/BkY8Ak997QauG0Iu2EqArv/Sourcegraph-Components?node-id=30%3A24',
        },
    }
)

add(
    'Cards',
    () => (
        <>
            <h1>Cards</h1>
            <p>
                A card is a flexible and extensible content container. It includes options for headers and footers, a
                wide variety of content, contextual background colors, and powerful display options.{' '}
                <a href="https://getbootstrap.com/docs/4.5/components/card/">Bootstrap documentation</a>
            </p>

            <h2>Examples</h2>

            <div className="card mb-3">
                <div className="card-body">This is some text within a card body.</div>
            </div>

            <div className="card mb-3" style={{ maxWidth: '18rem' }}>
                <div className="card-body">
                    <h3 className="card-title">Card title</h3>
                    <p className="card-text">
                        Some quick example text to build on the card title and make up the bulk of the card's content.
                    </p>
                    <button type="button" className="btn btn-primary">
                        Do something
                    </button>
                </div>
            </div>

            <div className="card">
                <div className="card-header">Featured</div>
                <div className="card-body">
                    <h3 className="card-title">Special title treatment</h3>
                    <p className="card-text">With supporting text below as a natural lead-in to additional content.</p>
                    <a href="https://example.com" target="_blank" rel="noopener noreferrer" className="btn btn-primary">
                        Go somewhere
                    </a>
                </div>
            </div>
        </>
    ),
    {
        design: {
            type: 'figma',
            url: 'https://www.figma.com/file/BkY8Ak997QauG0Iu2EqArv/Sourcegraph-Components?node-id=109%3A2',
        },
    }
)

add('List groups', () => (
    <>
        <h1>List groups</h1>
        <p>
            List groups are a flexible and powerful component for displaying a series of content. Modify and extend them
            to support just about any content within.
        </p>
        <ul className="list-group mb-3">
            <li className="list-group-item">Cras justo odio</li>
            <li className="list-group-item">Dapibus ac facilisis in</li>
            <li className="list-group-item">Morbi leo risus</li>
            <li className="list-group-item">Porta ac consectetur ac</li>
            <li className="list-group-item">Vestibulum at eros</li>
        </ul>

        <h2>Interactive</h2>
        <div className="list-group">
            <button
                type="button"
                className="list-group-item list-group-item-action active"
                onClick={action('List group item clicked')}
            >
                Cras justo odio
            </button>
            <button
                type="button"
                className="list-group-item list-group-item-action"
                onClick={action('List group item clicked')}
            >
                Dapibus ac facilisis in
            </button>
            <button
                type="button"
                className="list-group-item list-group-item-action"
                onClick={action('List group item clicked')}
            >
                Morbi leo risus
            </button>
            <button
                type="button"
                className="list-group-item list-group-item-action"
                onClick={action('List group item clicked')}
            >
                Porta ac consectetur ac
            </button>
            <button
                type="button"
                className="list-group-item list-group-item-action disabled"
                tabIndex={-1}
                aria-disabled="true"
                onClick={action('List group item clicked')}
            >
                Disabled
            </button>
        </div>
    </>
))
