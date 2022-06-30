import { FunctionComponent, useMemo } from 'react'

import { mdiTimerSand, mdiCheck, mdiAlertCircle, mdiProgressClock } from '@mdi/js'

import { isDefined } from '@sourcegraph/common'
import { LSIFIndexState } from '@sourcegraph/shared/src/graphql-operations'
import { Icon } from '@sourcegraph/wildcard'

import { ExecutionLogEntry } from '../../../../components/ExecutionLogEntry'
import { Timeline, TimelineStage } from '../../../../components/Timeline'
import { LsifIndexFields } from '../../../../graphql-operations'

import { ExecutionMetaInformation } from './ExecutionMetaInformation'

export interface CodeIntelIndexTimelineProps {
    index: LsifIndexFields
    now?: () => Date
    className?: string
}

export const CodeIntelIndexTimeline: FunctionComponent<React.PropsWithChildren<CodeIntelIndexTimelineProps>> = ({
    index,
    now,
    className,
}) => {
    const stages = useMemo(
        () =>
            [
                {
                    icon: <Icon aria-label="Success" svgPath={mdiTimerSand} />,
                    text: 'Queued',
                    date: index.queuedAt,
                    className: 'bg-success',
                },
                {
                    icon: <Icon aria-label="Success" svgPath={mdiCheck} />,
                    text: 'Began processing',
                    date: index.startedAt,
                    className: 'bg-success',
                },

                indexSetupStage(index, now),
                indexPreIndexStage(index, now),
                indexIndexStage(index, now),
                indexUploadStage(index, now),
                indexTeardownStage(index, now),

                index.state === LSIFIndexState.COMPLETED
                    ? {
                          icon: <Icon aria-label="Success" svgPath={mdiCheck} />,
                          text: 'Finished',
                          date: index.finishedAt,
                          className: 'bg-success',
                      }
                    : {
                          icon: <Icon aria-label="Failed" svgPath={mdiAlertCircle} />,
                          text: 'Failed',
                          date: index.finishedAt,
                          className: 'bg-danger',
                      },
            ]
                .filter(isDefined)
                .filter<TimelineStage>((stage): stage is TimelineStage => stage.date !== null),
        [index, now]
    )

    return <Timeline stages={stages} now={now} className={className} />
}

const indexSetupStage = (index: LsifIndexFields, now?: () => Date): TimelineStage | undefined =>
    index.steps.setup.length === 0
        ? undefined
        : {
              text: 'Setup',
              details: index.steps.setup.map(logEntry => (
                  <ExecutionLogEntry key={logEntry.key} logEntry={logEntry} now={now} />
              )),
              ...genericStage(index.steps.setup),
          }

const indexPreIndexStage = (index: LsifIndexFields, now?: () => Date): TimelineStage | undefined => {
    const logEntries = index.steps.preIndex.map(step => step.logEntry).filter(isDefined)

    return logEntries.length === 0
        ? undefined
        : {
              text: 'Pre Index',
              details: index.steps.preIndex.map(
                  step =>
                      step.logEntry && (
                          <div key={`${step.image}${step.root}${step.commands.join(' ')}}`}>
                              <ExecutionLogEntry logEntry={step.logEntry} now={now}>
                                  <ExecutionMetaInformation
                                      {...{
                                          image: step.image,
                                          commands: step.commands,
                                          root: step.root,
                                      }}
                                  />
                              </ExecutionLogEntry>
                          </div>
                      )
              ),
              ...genericStage(logEntries),
          }
}

const indexIndexStage = (index: LsifIndexFields, now?: () => Date): TimelineStage | undefined =>
    !index.steps.index.logEntry
        ? undefined
        : {
              text: 'Index',
              details: (
                  <>
                      <ExecutionLogEntry logEntry={index.steps.index.logEntry} now={now}>
                          <ExecutionMetaInformation
                              {...{
                                  image: index.inputIndexer,
                                  commands: index.steps.index.indexerArgs,
                                  root: index.inputRoot,
                              }}
                          />
                      </ExecutionLogEntry>
                  </>
              ),
              ...genericStage(index.steps.index.logEntry),
          }

const indexUploadStage = (index: LsifIndexFields, now?: () => Date): TimelineStage | undefined =>
    !index.steps.upload
        ? undefined
        : {
              text: 'Upload',
              details: <ExecutionLogEntry logEntry={index.steps.upload} now={now} />,
              ...genericStage(index.steps.upload),
          }

const indexTeardownStage = (index: LsifIndexFields, now?: () => Date): TimelineStage | undefined =>
    index.steps.teardown.length === 0
        ? undefined
        : {
              text: 'Teardown',
              details: index.steps.teardown.map(logEntry => (
                  <ExecutionLogEntry key={logEntry.key} logEntry={logEntry} now={now} />
              )),
              ...genericStage(index.steps.teardown),
          }

const genericStage = <E extends { startTime: string; exitCode: number | null }>(
    value: E | E[]
): Pick<TimelineStage, 'icon' | 'date' | 'className' | 'expandedByDefault'> => {
    const finished = Array.isArray(value)
        ? value.every(logEntry => logEntry.exitCode !== null)
        : value.exitCode !== null
    const success = Array.isArray(value) ? value.every(logEntry => logEntry.exitCode === 0) : value.exitCode === 0

    return {
        icon: !finished ? (
            <Icon aria-label="Success" svgPath={mdiProgressClock} />
        ) : success ? (
            <Icon aria-label="Success" svgPath={mdiCheck} />
        ) : (
            <Icon aria-label="Failed" svgPath={mdiAlertCircle} />
        ),
        date: Array.isArray(value) ? value[0].startTime : value.startTime,
        className: success || !finished ? 'bg-success' : 'bg-danger',
        expandedByDefault: !(success || !finished),
    }
}
