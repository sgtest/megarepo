import { BatchChangesIconNamespaceNav } from '../../batches/icons'
import type { NamespaceAreaNavItem } from '../../namespaces/NamespaceArea'

export const enterpriseNamespaceAreaHeaderNavItems: readonly NamespaceAreaNavItem[] = [
    {
        to: '/batch-changes',
        label: 'Batch Changes',
        icon: BatchChangesIconNamespaceNav,
        condition: ({ batchChangesEnabled, isSourcegraphApp }) => batchChangesEnabled && !isSourcegraphApp,
    },
]
