import Dialog from '@reach/dialog'
import classNames from 'classnames'
import CloseIcon from 'mdi-react/CloseIcon'
import React, { useCallback, useState } from 'react'

import { Button } from '@sourcegraph/wildcard'

import styles from './ModalVideo.module.scss'

interface ModalVideoProps {
    id: string
    title: string
    src: string
    thumbnail: { src: string; alt: string }
    onToggle?: (isOpen: boolean) => void
    showCaption?: boolean
    className?: string
}

export const ModalVideo: React.FunctionComponent<ModalVideoProps> = ({
    id,
    title,
    src,
    thumbnail,
    onToggle,
    showCaption = false,
    className,
}) => {
    const assetsRoot = window.context?.assetsRoot || ''
    const [isOpen, setIsOpen] = useState(false)
    const toggleDialog = useCallback(
        isOpen => {
            setIsOpen(isOpen)
            if (onToggle) {
                onToggle(isOpen)
            }
        },
        [onToggle]
    )

    let thumbnailElement = (
        <button type="button" className={styles.thumbnailButton} onClick={() => toggleDialog(true)}>
            <img src={`${assetsRoot}/${thumbnail.src}`} alt={thumbnail.alt} className={styles.thumbnailImage} />
            <div className={styles.playIconWrapper}>
                <PlayIcon />
            </div>
        </button>
    )

    if (showCaption) {
        thumbnailElement = (
            <figure>
                {thumbnailElement}
                <figcaption>
                    <Button
                        variant="link"
                        className="font-weight-normal p-0 pt-2 w-100"
                        onClick={() => toggleDialog(true)}
                    >
                        {title}
                    </Button>
                </figcaption>
            </figure>
        )
    }

    return (
        <>
            <div className={classNames(styles.wrapper, className)}>{thumbnailElement}</div>
            {isOpen && (
                <Dialog
                    className={classNames(styles.modal, 'modal-body modal-body--centered p-4 rounded border')}
                    onDismiss={() => toggleDialog(false)}
                    aria-labelledby={id}
                >
                    <div className={styles.modalContent}>
                        <div className={styles.modalHeader}>
                            <h3 id={id}>{title}</h3>
                            <Button className="btn-icon p-1" onClick={() => toggleDialog(false)} aria-label="Close">
                                <CloseIcon className="icon-inline" />
                            </Button>
                        </div>
                        <div className="w-100">
                            <div className={styles.iframeVideoWrapper}>
                                <iframe
                                    className={styles.iframeVideo}
                                    src={src}
                                    title="YouTube video player"
                                    frameBorder="0"
                                    allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
                                    allowFullScreen={true}
                                />
                            </div>
                        </div>
                    </div>
                </Dialog>
            )}
        </>
    )
}

const PlayIcon = React.memo(() => (
    <svg width="50" height="53" viewBox="0 0 50 53" fill="none" xmlns="http://www.w3.org/2000/svg">
        <g filter="url(#filter0_dd_268:5695)">
            <path d="M37.5 26.5L12.75 40.7894L12.75 12.2106L37.5 26.5Z" fill="white" />
        </g>
        <defs>
            <filter
                id="filter0_dd_268:5695"
                x="0.75"
                y="0.210449"
                width="48.75"
                height="52.5791"
                filterUnits="userSpaceOnUse"
                colorInterpolationFilters="sRGB"
            >
                <feFlood floodOpacity="0" result="BackgroundImageFix" />
                <feColorMatrix
                    in="SourceAlpha"
                    type="matrix"
                    values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 127 0"
                    result="hardAlpha"
                />
                <feOffset />
                <feGaussianBlur stdDeviation="6" />
                <feColorMatrix
                    type="matrix"
                    values="0 0 0 0 0.00505209 0 0 0 0 0.0449636 0 0 0 0 0.404167 0 0 0 0.25 0"
                />
                <feBlend mode="normal" in2="BackgroundImageFix" result="effect1_dropShadow_268:5695" />
                <feColorMatrix
                    in="SourceAlpha"
                    type="matrix"
                    values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 127 0"
                    result="hardAlpha"
                />
                <feOffset dy="4" />
                <feGaussianBlur stdDeviation="2" />
                <feColorMatrix type="matrix" values="0 0 0 0 0 0 0 0 0 0.055 0 0 0 0 0.25 0 0 0 0.25 0" />
                <feBlend mode="normal" in2="effect1_dropShadow_268:5695" result="effect2_dropShadow_268:5695" />
                <feBlend mode="normal" in="SourceGraphic" in2="effect2_dropShadow_268:5695" result="shape" />
            </filter>
        </defs>
    </svg>
))
