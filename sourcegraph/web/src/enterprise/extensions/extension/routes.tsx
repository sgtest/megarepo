import React from 'react'
import { ExtensionAreaRoute } from '../../../extensions/extension/ExtensionArea'
import { extensionAreaRoutes } from '../../../extensions/extension/routes'
const RegistryExtensionManagePage = React.lazy(async () => ({
    default: (await import('./RegistryExtensionManagePage')).RegistryExtensionManagePage,
}))
const RegistryExtensionNewReleasePage = React.lazy(async () => ({
    default: (await import('./RegistryExtensionNewReleasePage')).RegistryExtensionNewReleasePage,
}))

export const enterpriseExtensionAreaRoutes: ReadonlyArray<ExtensionAreaRoute> = [
    ...extensionAreaRoutes,
    {
        path: `/-/manage`,
        exact: true,
        // tslint:disable-next-line:jsx-no-lambda
        render: props => <RegistryExtensionManagePage {...props} />,
    },
    {
        path: `/-/releases/new`,
        exact: true,
        // tslint:disable-next-line:jsx-no-lambda
        render: props => <RegistryExtensionNewReleasePage {...props} />,
    },
]
