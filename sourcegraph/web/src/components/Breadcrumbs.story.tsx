import { storiesOf } from '@storybook/react'
import React from 'react'
import { Breadcrumbs } from './Breadcrumbs'
import webStyles from '../SourcegraphWebApp.scss'

const { add } = storiesOf('web/Breadcrumbs', module).addDecorator(story => (
    <>
        <style>{webStyles}</style>
        <div className="container mt-3 theme-light">{story()}</div>
    </>
))

add(
    'Example',
    () => (
        <Breadcrumbs
            breadcrumbs={[
                {
                    depth: 0,
                    breadcrumb: { key: 'home', element: <a href="#">Home</a>, divider: null },
                },
                {
                    depth: 1,
                    breadcrumb: { key: 'home', element: <a href="#">Repositories</a> },
                },
                {
                    depth: 2,
                    breadcrumb: {
                        key: 'repo',
                        element: (
                            <a href="#">
                                sourcegraph/<span className="font-weight-semibold">sourcegraph</span>
                            </a>
                        ),
                    },
                },
                {
                    depth: 3,
                    breadcrumb: {
                        key: 'revision',
                        divider: <span className="mx-1">@</span>,
                        element: <span className="text-muted">fb/my-branch</span>,
                    },
                },
                {
                    depth: 4,
                    breadcrumb: { key: 'directory1', element: <a href="#">path</a> },
                },
                {
                    depth: 5,
                    breadcrumb: {
                        key: 'directory2',
                        divider: <span className="mx-1">/</span>,
                        element: <a href="#">to</a>,
                    },
                },
                {
                    depth: 6,
                    breadcrumb: {
                        key: 'fileName',
                        divider: <span className="mx-1">/</span>,
                        element: <a href="#">file.tsx</a>,
                    },
                },
            ]}
        />
    ),
    {
        design: {
            type: 'figma',
            url: 'https://www.figma.com/file/BkY8Ak997QauG0Iu2EqArv/Sourcegraph-Components?node-id=230%3A83',
        },
    }
)
