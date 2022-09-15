import { DecoratorFn, Meta, Story } from '@storybook/react'
import { of } from 'rxjs'

import { WebStory } from '../../../../../components/WebStory'
import { mockWorkspaces } from '../../batch-spec.mock'
import { queryWorkspacesList as _queryWorkspacesList } from '../backend'

import { Workspaces } from './Workspaces'

const decorator: DecoratorFn = story => <div className="p-3 container">{story()}</div>

const config: Meta = {
    title: 'web/batches/batch-spec/execute/workspaces/Workspaces',
    decorators: [decorator],
}

export default config

const queryWorkspacesList: typeof _queryWorkspacesList = () =>
    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
    of(mockWorkspaces(50).node.workspaceResolution!.workspaces)

export const WorkspacesStory: Story = () => (
    <WebStory>
        {props => (
            <Workspaces
                batchSpecID="1"
                selectedNode="workspace1"
                executionURL=""
                queryWorkspacesList={queryWorkspacesList}
                {...props}
            />
        )}
    </WebStory>
)

WorkspacesStory.storyName = 'Workspaces'
