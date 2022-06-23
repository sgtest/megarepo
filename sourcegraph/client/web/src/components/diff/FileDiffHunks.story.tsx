import { boolean } from '@storybook/addon-knobs'
import { DecoratorFn, Meta, Story } from '@storybook/react'

import { FileDiffHunkFields, DiffHunkLineType } from '../../graphql-operations'
import { WebStory } from '../WebStory'

import { FileDiffHunks } from './FileDiffHunks'

export const DEMO_HUNKS: FileDiffHunkFields[] = [
    {
        oldRange: { lines: 7, startLine: 3 },
        newRange: { lines: 7, startLine: 3 },
        oldNoNewlineAt: false,
        section: 'func awesomeness(param string) (int, error) {',
        highlight: {
            aborted: false,
            lines: [
                {
                    kind: DiffHunkLineType.UNCHANGED,
                    html: '    v, err := makeAwesome()',
                },
                {
                    kind: DiffHunkLineType.UNCHANGED,
                    html: '    if err != nil {',
                },
                {
                    kind: DiffHunkLineType.UNCHANGED,
                    html: '        fmt.Printf("wow: %v", err)',
                },
                {
                    kind: DiffHunkLineType.DELETED,
                    html: '        return err',
                },
                {
                    kind: DiffHunkLineType.ADDED,
                    html: '        return nil, err',
                },
                {
                    kind: DiffHunkLineType.UNCHANGED,
                    html: '    }',
                },
                {
                    kind: DiffHunkLineType.UNCHANGED,
                    html: '    return v.Score, nil',
                },
                {
                    kind: DiffHunkLineType.UNCHANGED,
                    html: '}',
                },
            ],
        },
    },
]

const decorator: DecoratorFn = story => <div className="p-3 container">{story()}</div>

const config: Meta = {
    title: 'web/diffs/FileDiffHunks',
    decorators: [decorator],
    includeStories: ['OneDiffUnifiedHunk', 'OneDiffSplitHunk'],
}

export default config

export const OneDiffUnifiedHunk: Story = () => (
    <WebStory>
        {webProps => (
            <FileDiffHunks
                diffMode="unified"
                {...webProps}
                persistLines={boolean('persistLines', true)}
                fileDiffAnchor="abc"
                lineNumbers={boolean('lineNumbers', true)}
                hunks={DEMO_HUNKS}
                className="abcdef"
            />
        )}
    </WebStory>
)

OneDiffUnifiedHunk.storyName = 'One diff unified hunk'

export const OneDiffSplitHunk: Story = () => (
    <WebStory>
        {webProps => (
            <FileDiffHunks
                diffMode="split"
                {...webProps}
                persistLines={boolean('persistLines', true)}
                fileDiffAnchor="abc"
                lineNumbers={boolean('lineNumbers', true)}
                hunks={DEMO_HUNKS}
                className="abcdef"
            />
        )}
    </WebStory>
)

OneDiffSplitHunk.storyName = 'One diff split hunk'
