import { css, cx } from '@emotion/css';
import { isEqual, pickBy } from 'lodash';
import React, { useEffect, useMemo, useState } from 'react';
import { FormProvider, useForm } from 'react-hook-form';
import { useDebounce } from 'react-use';

import {
  addDurationToDate,
  dateTime,
  DefaultTimeZone,
  GrafanaTheme2,
  intervalToAbbreviatedDurationString,
  isValidDate,
  parseDuration,
} from '@grafana/data';
import { config, isFetchError, locationService } from '@grafana/runtime';
import {
  Alert,
  Button,
  Field,
  FieldSet,
  Input,
  LinkButton,
  LoadingPlaceholder,
  TextArea,
  useStyles2,
} from '@grafana/ui';
import { alertSilencesApi } from 'app/features/alerting/unified/api/alertSilencesApi';
import { getDatasourceAPIUid } from 'app/features/alerting/unified/utils/datasource';
import { Matcher, MatcherOperator, Silence, SilenceCreatePayload } from 'app/plugins/datasource/alertmanager/types';

import { useURLSearchParams } from '../../hooks/useURLSearchParams';
import { SilenceFormFields } from '../../types/silence-form';
import { matcherFieldToMatcher, matcherToMatcherField } from '../../utils/alertmanager';
import { parseQueryParamMatchers } from '../../utils/matchers';
import { makeAMLink } from '../../utils/misc';

import MatchersField from './MatchersField';
import { SilencePeriod } from './SilencePeriod';
import { SilencedInstancesPreview } from './SilencedInstancesPreview';

interface Props {
  silenceId?: string;
  alertManagerSourceName: string;
}

const defaultsFromQuery = (searchParams: URLSearchParams): Partial<SilenceFormFields> => {
  const defaults: Partial<SilenceFormFields> = {};

  const comment = searchParams.get('comment');
  const matchers = searchParams.getAll('matcher');

  const formMatchers = parseQueryParamMatchers(matchers);
  if (formMatchers.length) {
    defaults.matchers = formMatchers.map(matcherToMatcherField);
  }

  if (comment) {
    defaults.comment = comment;
  }

  return defaults;
};

const getDefaultFormValues = (searchParams: URLSearchParams, silence?: Silence): SilenceFormFields => {
  const now = new Date();
  if (silence) {
    const isExpired = Date.parse(silence.endsAt) < Date.now();
    const interval = isExpired
      ? {
          start: now,
          end: addDurationToDate(now, { hours: 2 }),
        }
      : { start: new Date(silence.startsAt), end: new Date(silence.endsAt) };
    return {
      id: silence.id,
      startsAt: interval.start.toISOString(),
      endsAt: interval.end.toISOString(),
      comment: silence.comment,
      createdBy: silence.createdBy,
      duration: intervalToAbbreviatedDurationString(interval),
      isRegex: false,
      matchers: silence.matchers?.map(matcherToMatcherField) || [],
      matcherName: '',
      matcherValue: '',
      timeZone: DefaultTimeZone,
    };
  } else {
    const endsAt = addDurationToDate(now, { hours: 2 }); // Default time period is now + 2h
    return {
      id: '',
      startsAt: now.toISOString(),
      endsAt: endsAt.toISOString(),
      comment: `created ${dateTime().format('YYYY-MM-DD HH:mm')}`,
      createdBy: config.bootData.user.name,
      duration: '2h',
      isRegex: false,
      matchers: [{ name: '', value: '', operator: MatcherOperator.equal }],
      matcherName: '',
      matcherValue: '',
      timeZone: DefaultTimeZone,
      ...defaultsFromQuery(searchParams),
    };
  }
};

export const SilencesEditor = ({ silenceId, alertManagerSourceName }: Props) => {
  // Use a lazy query to fetch the Silence info, as we may not always require this
  // (e.g. if creating a new one from scratch, we don't need to fetch anything)
  const [getSilence, { data: silence, isLoading: getSilenceIsLoading, error: errorGettingExistingSilence }] =
    alertSilencesApi.endpoints.getSilence.useLazyQuery();
  const [createSilence, { isLoading }] = alertSilencesApi.endpoints.createSilence.useMutation();
  const [urlSearchParams] = useURLSearchParams();

  const defaultValues = useMemo(() => getDefaultFormValues(urlSearchParams, silence), [silence, urlSearchParams]);
  const formAPI = useForm({ defaultValues });

  const styles = useStyles2(getStyles);
  const [matchersForPreview, setMatchersForPreview] = useState<Matcher[]>(
    defaultValues.matchers.map(matcherFieldToMatcher)
  );

  const { register, handleSubmit, formState, watch, setValue, clearErrors, reset } = formAPI;

  const onSubmit = async (data: SilenceFormFields) => {
    const { id, startsAt, endsAt, comment, createdBy, matchers: matchersFields } = data;
    const matchers = matchersFields.map(matcherFieldToMatcher);
    const payload = pickBy(
      {
        id,
        startsAt,
        endsAt,
        comment,
        createdBy,
        matchers,
      },
      (value) => !!value
    ) as SilenceCreatePayload;
    await createSilence({ datasourceUid: getDatasourceAPIUid(alertManagerSourceName), payload })
      .unwrap()
      .then(() => {
        locationService.push(makeAMLink('/alerting/silences', alertManagerSourceName));
      });
  };

  const duration = watch('duration');
  const startsAt = watch('startsAt');
  const endsAt = watch('endsAt');
  const matcherFields = watch('matchers');

  useEffect(() => {
    if (silence) {
      // Allows the form to correctly initialise when an existing silence is fetch from the backend
      reset(getDefaultFormValues(urlSearchParams, silence));
    }
  }, [reset, silence, urlSearchParams]);

  useEffect(() => {
    if (silenceId) {
      getSilence({ id: silenceId, datasourceUid: getDatasourceAPIUid(alertManagerSourceName) });
    }
  }, [alertManagerSourceName, getSilence, silenceId]);

  // Keep duration and endsAt in sync
  const [prevDuration, setPrevDuration] = useState(duration);
  useDebounce(
    () => {
      if (isValidDate(startsAt) && isValidDate(endsAt)) {
        if (duration !== prevDuration) {
          setValue('endsAt', dateTime(addDurationToDate(new Date(startsAt), parseDuration(duration))).toISOString());
          setPrevDuration(duration);
        } else {
          const startValue = new Date(startsAt).valueOf();
          const endValue = new Date(endsAt).valueOf();
          if (endValue > startValue) {
            const nextDuration = intervalToAbbreviatedDurationString({
              start: new Date(startsAt),
              end: new Date(endsAt),
            });
            setValue('duration', nextDuration);
            setPrevDuration(nextDuration);
          }
        }
      }
    },
    700,
    [clearErrors, duration, endsAt, prevDuration, setValue, startsAt]
  );

  useDebounce(
    () => {
      // React-hook-form watch does not return referentialy equal values so this trick is needed
      const newMatchers = matcherFields.filter((m) => m.name && m.value).map(matcherFieldToMatcher);
      if (!isEqual(matchersForPreview, newMatchers)) {
        setMatchersForPreview(newMatchers);
      }
    },
    700,
    [matcherFields]
  );

  const userLogged = Boolean(config.bootData.user.isSignedIn && config.bootData.user.name);

  if (getSilenceIsLoading) {
    return <LoadingPlaceholder text="Loading existing silence information..." />;
  }

  const existingSilenceNotFound =
    isFetchError(errorGettingExistingSilence) && errorGettingExistingSilence.status === 404;

  if (existingSilenceNotFound) {
    return <Alert title={`Existing silence "${silenceId}" not found`} severity="warning" />;
  }

  return (
    <FormProvider {...formAPI}>
      <form onSubmit={handleSubmit(onSubmit)}>
        <FieldSet>
          <div className={cx(styles.flexRow, styles.silencePeriod)}>
            <SilencePeriod />
            <Field
              label="Duration"
              invalid={!!formState.errors.duration}
              error={
                formState.errors.duration &&
                (formState.errors.duration.type === 'required' ? 'Required field' : formState.errors.duration.message)
              }
            >
              <Input
                className={styles.createdBy}
                {...register('duration', {
                  validate: (value) =>
                    Object.keys(parseDuration(value)).length === 0
                      ? 'Invalid duration. Valid example: 1d 4h (Available units: y, M, w, d, h, m, s)'
                      : undefined,
                })}
                id="duration"
              />
            </Field>
          </div>

          <MatchersField />
          <Field
            className={cx(styles.field, styles.textArea)}
            label="Comment"
            required
            error={formState.errors.comment?.message}
            invalid={!!formState.errors.comment}
          >
            <TextArea
              {...register('comment', { required: { value: true, message: 'Required.' } })}
              rows={5}
              placeholder="Details about the silence"
            />
          </Field>
          {!userLogged && (
            <Field
              className={cx(styles.field, styles.createdBy)}
              label="Created By"
              required
              error={formState.errors.createdBy?.message}
              invalid={!!formState.errors.createdBy}
            >
              <Input
                {...register('createdBy', { required: { value: true, message: 'Required.' } })}
                placeholder="Who's creating the silence"
              />
            </Field>
          )}
          <SilencedInstancesPreview amSourceName={alertManagerSourceName} matchers={matchersForPreview} />
        </FieldSet>
        <div className={styles.flexRow}>
          {isLoading && (
            <Button disabled={true} icon="spinner" variant="primary">
              Saving...
            </Button>
          )}
          {!isLoading && <Button type="submit">Save silence</Button>}
          <LinkButton href={makeAMLink('alerting/silences', alertManagerSourceName)} variant={'secondary'}>
            Cancel
          </LinkButton>
        </div>
      </form>
    </FormProvider>
  );
};

const getStyles = (theme: GrafanaTheme2) => ({
  field: css({
    margin: theme.spacing(1, 0),
  }),
  textArea: css({
    maxWidth: `${theme.breakpoints.values.sm}px`,
  }),
  createdBy: css({
    width: '200px',
  }),
  flexRow: css({
    display: 'flex',
    flexDirection: 'row',
    justifyContent: 'flex-start',

    '& > *': {
      marginRight: theme.spacing(1),
    },
  }),
  silencePeriod: css({
    maxWidth: `${theme.breakpoints.values.sm}px`,
  }),
});

export default SilencesEditor;
