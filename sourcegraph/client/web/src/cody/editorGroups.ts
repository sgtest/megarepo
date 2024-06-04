import type { IEditor } from './onboarding/CodyOnboarding'
import { JetBrainsInstructions } from './onboarding/instructions/JetBrains'
import { NeoVimInstructions } from './onboarding/instructions/NeoVim'
import { VSCodeInstructions } from './onboarding/instructions/VsCode'

export const editorGroups: IEditor[][] = [
    [
        {
            id: 1,
            icon: 'VsCode',
            name: 'VS Code',
            publisher: 'Microsoft',
            releaseStage: 'Stable',
            docs: 'https://sourcegraph.com/docs/cody/clients/install-vscode',
            instructions: VSCodeInstructions,
        },
        {
            id: 2,
            icon: 'IntelliJ',
            name: 'IntelliJ IDEA',
            publisher: 'JetBrains',
            releaseStage: 'Stable',
            docs: 'https://sourcegraph.com/docs/cody/clients/install-jetbrains',
            instructions: JetBrainsInstructions,
        },
        {
            id: 3,
            icon: 'PhpStorm',
            name: 'PhpStorm ',
            publisher: 'JetBrains',
            releaseStage: 'Stable',
            docs: 'https://sourcegraph.com/docs/cody/clients/install-jetbrains',
            instructions: JetBrainsInstructions,
        },
        {
            id: 4,
            icon: 'PyCharm',
            name: 'PyCharm',
            publisher: 'JetBrains',
            releaseStage: 'Stable',
            docs: 'https://sourcegraph.com/docs/cody/clients/install-jetbrains',
            instructions: JetBrainsInstructions,
        },
    ],
    [
        {
            id: 5,
            icon: 'WebStorm',
            name: 'WebStorm',
            publisher: 'JetBrains',
            releaseStage: 'Stable',
            docs: 'https://sourcegraph.com/docs/cody/clients/install-jetbrains',
            instructions: JetBrainsInstructions,
        },
        {
            id: 6,
            icon: 'RubyMine',
            name: 'RubyMine',
            publisher: 'JetBrains',
            releaseStage: 'Stable',
            docs: 'https://sourcegraph.com/docs/cody/clients/install-jetbrains',
            instructions: JetBrainsInstructions,
        },
        {
            id: 7,
            icon: 'GoLand',
            name: 'GoLand',
            publisher: 'JetBrains',
            releaseStage: 'Stable',
            docs: 'https://sourcegraph.com/docs/cody/clients/install-jetbrains',
            instructions: JetBrainsInstructions,
        },
        {
            id: 8,
            icon: 'AndroidStudio',
            name: 'Android Studio',
            publisher: 'Google',
            releaseStage: 'Stable',
            docs: 'https://sourcegraph.com/docs/cody/clients/install-jetbrains',
            instructions: JetBrainsInstructions,
        },
    ],
    [
        {
            id: 9,
            icon: 'NeoVim',
            name: 'Neovim',
            publisher: 'Neovim Team',
            releaseStage: 'Experimental',
            docs: 'https://sourcegraph.com/docs/cody/clients/install-neovim',
            instructions: NeoVimInstructions,
        },
    ],
]
