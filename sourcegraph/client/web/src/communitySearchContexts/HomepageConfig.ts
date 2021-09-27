import { cncf } from './cncf'
import { kubernetes } from './Kubernetes'
import { o3de } from './o3de'
import { stackStorm } from './StackStorm'
import { stanford } from './Stanford'
import { temporal } from './Temporal'
import { CommunitySearchContextMetadata } from './types'

export const communitySearchContextsList: CommunitySearchContextMetadata[] = [
    cncf,
    temporal,
    o3de,
    stackStorm,
    kubernetes,
    stanford,
]
