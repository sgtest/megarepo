import React, { memo } from 'react'

import type { MdiReactIconProps } from 'mdi-react'

export const CircleDashedIcon: React.FunctionComponent<React.PropsWithChildren<MdiReactIconProps>> = memo(
    ({ color = 'currentColor', size = 24, className = '', ...props }) => (
        <svg {...props} className={className} width={size} height={size} fill={color} viewBox="0 0 24 24">
            <path d="M10.59 1.44l.17 1.3a9.4 9.4 0 012.8.04l.21-1.28c-1.05-.18-2.12-.2-3.18-.06zM8.32 2c-1 .37-1.95.9-2.8 1.54l.8 1.03a9.28 9.28 0 012.45-1.35L8.32 2zm7.7.13l-.49 1.21a9.3 9.3 0 012.4 1.43l.82-1a10.59 10.59 0 00-2.73-1.64zM3.84 5.15a10.6 10.6 0 00-1.66 2.72l1.2.5A9.3 9.3 0 014.84 6l-1-.84zm16.55.29l-1.02.8a9.28 9.28 0 011.37 2.43l1.22-.46c-.38-1-.91-1.93-1.57-2.77zM1.52 10.12a10.72 10.72 0 00-.1 3.17l1.3-.15a9.42 9.42 0 01.08-2.8l-1.28-.22zm21.02.35l-1.29.19a9.26 9.26 0 01-.08 3.2l1.27.26a10.65 10.65 0 00.1-3.65zM3.2 15.14l-1.23.43c.36 1 .87 1.95 1.5 2.8l1.05-.77c-.56-.75-1.01-1.58-1.32-2.46zm17.35.67c-.38.86-.9 1.65-1.5 2.35l.97.86c.7-.8 1.28-1.7 1.72-2.68l-1.2-.53zM5.9 19.1l-.84.98c.8.7 1.72 1.27 2.7 1.7l.51-1.2a9.3 9.3 0 01-2.37-1.48zm11.6.45c-.75.55-1.59 1-2.47 1.3l.42 1.22c1.01-.34 1.96-.84 2.82-1.47l-.77-1.05zm-7.27 1.63L10 22.46c1.05.2 2.12.24 3.18.12l-.14-1.29c-.93.1-1.88.07-2.8-.1z" />
        </svg>
    )
)
