import { StoryFn } from '@storybook/addons'
import React, { ReactElement } from 'react'

export const CardsStory: StoryFn<ReactElement> = () => (
    <>
        <h1>Cards</h1>
        <p>
            A card is a flexible and extensible content container. It includes options for headers and footers, a wide
            variety of content, contextual background colors, and powerful display options.{' '}
            <a href="https://getbootstrap.com/docs/4.5/components/card/">Bootstrap documentation</a>
        </p>

        <h2>Examples</h2>

        <div className="card mb-3">
            <div className="card-body">This is some text within a card body.</div>
        </div>

        {/* eslint-disable-next-line react/forbid-dom-props */}
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
)
