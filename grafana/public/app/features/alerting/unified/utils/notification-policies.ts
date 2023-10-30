import { isArray, merge, pick, reduce } from 'lodash';

import {
  AlertmanagerGroup,
  MatcherOperator,
  ObjectMatcher,
  Route,
  RouteWithID,
} from 'app/plugins/datasource/alertmanager/types';
import { Labels } from 'app/types/unified-alerting-dto';

import { Label, normalizeMatchers } from './matchers';

// If a policy has no matchers it still can be a match, hence matchers can be empty and match can be true
// So we cannot use null as an indicator of no match
interface LabelMatchResult {
  match: boolean;
  matcher: ObjectMatcher | null;
}

type LabelsMatch = Map<Label, LabelMatchResult>;

interface MatchingResult {
  matches: boolean;
  labelsMatch: LabelsMatch;
}

// returns a match results for given set of matchers (from a policy for instance) and a set of labels
export function matchLabels(matchers: ObjectMatcher[], labels: Label[]): MatchingResult {
  const matches = matchLabelsSet(matchers, labels);

  // create initial map of label => match result
  const labelsMatch: LabelsMatch = new Map(labels.map((label) => [label, { match: false, matcher: null }]));

  // for each matcher, check which label it matched for
  matchers.forEach((matcher) => {
    const matchingLabel = labels.find((label) => isLabelMatch(matcher, label));

    // record that matcher for the label
    if (matchingLabel) {
      labelsMatch.set(matchingLabel, {
        match: true,
        matcher,
      });
    }
  });

  return { matches, labelsMatch };
}

// Compare set of matchers to set of label
export function matchLabelsSet(matchers: ObjectMatcher[], labels: Label[]): boolean {
  for (const matcher of matchers) {
    if (!isLabelMatchInSet(matcher, labels)) {
      return false;
    }
  }
  return true;
}

export interface AlertInstanceMatch {
  instance: Labels;
  labelsMatch: LabelsMatch;
}

export interface RouteMatchResult<T extends Route> {
  route: T;
  labelsMatch: LabelsMatch;
}

// Match does a depth-first left-to-right search through the route tree
// and returns the matching routing nodes.

// If the current node is not a match, return nothing
// Normalization should have happened earlier in the code
function findMatchingRoutes<T extends Route>(route: T, labels: Label[]): Array<RouteMatchResult<T>> {
  let childMatches: Array<RouteMatchResult<T>> = [];

  // If the current node is not a match, return nothing
  const matchResult = matchLabels(route.object_matchers ?? [], labels);
  if (!matchResult.matches) {
    return [];
  }

  // If the current node matches, recurse through child nodes
  if (route.routes) {
    for (const child of route.routes) {
      let matchingChildren = findMatchingRoutes(child, labels);
      // TODO how do I solve this typescript thingy? It looks correct to me /shrug
      // @ts-ignore
      childMatches = childMatches.concat(matchingChildren);
      // we have matching children and we don't want to continue, so break here
      if (matchingChildren.length && !child.continue) {
        break;
      }
    }
  }

  // If no child nodes were matches, the current node itself is a match.
  if (childMatches.length === 0) {
    childMatches.push({ route, labelsMatch: matchResult.labelsMatch });
  }

  return childMatches;
}

// This is a performance improvement to normalize matchers only once and use the normalized version later on
export function normalizeRoute(rootRoute: RouteWithID): RouteWithID {
  function normalizeRoute(route: RouteWithID) {
    route.object_matchers = normalizeMatchers(route);
    delete route.matchers;
    delete route.match;
    delete route.match_re;
    route.routes?.forEach(normalizeRoute);
  }

  const normalizedRootRoute = structuredClone(rootRoute);
  normalizeRoute(normalizedRootRoute);

  return normalizedRootRoute;
}

/**
 * find all of the groups that have instances that match the route, thay way we can find all instances
 * (and their grouping) for the given route
 */
function findMatchingAlertGroups(
  routeTree: Route,
  route: Route,
  alertGroups: AlertmanagerGroup[]
): AlertmanagerGroup[] {
  const matchingGroups: AlertmanagerGroup[] = [];

  return alertGroups.reduce((acc, group) => {
    // find matching alerts in the current group
    const matchingAlerts = group.alerts.filter((alert) => {
      const labels = Object.entries(alert.labels);
      return findMatchingRoutes(routeTree, labels).some((matchingRoute) => matchingRoute.route === route);
    });

    // if the groups has any alerts left after matching, add it to the results
    if (matchingAlerts.length) {
      acc.push({
        ...group,
        alerts: matchingAlerts,
      });
    }

    return acc;
  }, matchingGroups);
}

export type InhertitableProperties = Pick<
  Route,
  'receiver' | 'group_by' | 'group_wait' | 'group_interval' | 'repeat_interval' | 'mute_time_intervals'
>;

// inherited properties are config properties that exist on the parent route (or its inherited properties) but not on the child route
function getInheritedProperties(
  parentRoute: Route,
  childRoute: Route,
  propertiesParentInherited?: Partial<InhertitableProperties>
) {
  const fullParentProperties = merge({}, parentRoute, propertiesParentInherited);

  const inheritableProperties: InhertitableProperties = pick(fullParentProperties, [
    'receiver',
    'group_by',
    'group_wait',
    'group_interval',
    'repeat_interval',
    'mute_time_intervals',
  ]);

  // TODO how to solve this TypeScript mystery?
  const inherited = reduce(
    inheritableProperties,
    (inheritedProperties: Partial<Route> = {}, parentValue, property) => {
      const parentHasValue = parentValue !== undefined;

      // @ts-ignore
      const inheritFromParentUndefined = parentHasValue && childRoute[property] === undefined;
      // @ts-ignore
      const inheritFromParentEmptyString = parentHasValue && childRoute[property] === '';

      const inheritEmptyGroupByFromParent =
        property === 'group_by' &&
        parentHasValue &&
        isArray(childRoute[property]) &&
        childRoute[property]?.length === 0;

      const inheritFromParent =
        inheritFromParentUndefined || inheritFromParentEmptyString || inheritEmptyGroupByFromParent;

      if (inheritFromParent) {
        // @ts-ignore
        inheritedProperties[property] = parentValue;
      }

      return inheritedProperties;
    },
    {}
  );

  return inherited;
}

/**
 * This function will compute the full tree with inherited properties – this is mostly used for search and filtering
 */
export function computeInheritedTree<T extends Route>(parent: T): T {
  return {
    ...parent,
    routes: parent.routes?.map((child) => {
      const inheritedProperties = getInheritedProperties(parent, child);

      return computeInheritedTree({
        ...child,
        ...inheritedProperties,
      });
    }),
  };
}

type OperatorPredicate = (labelValue: string, matcherValue: string) => boolean;
const OperatorFunctions: Record<MatcherOperator, OperatorPredicate> = {
  [MatcherOperator.equal]: (lv, mv) => lv === mv,
  [MatcherOperator.notEqual]: (lv, mv) => lv !== mv,
  [MatcherOperator.regex]: (lv, mv) => new RegExp(mv).test(lv),
  [MatcherOperator.notRegex]: (lv, mv) => !new RegExp(mv).test(lv),
};

function isLabelMatchInSet(matcher: ObjectMatcher, labels: Label[]): boolean {
  const [matcherKey, operator, matcherValue] = matcher;

  let labelValue = ''; // matchers that have no labels are treated as empty string label values
  const labelForMatcher = Object.fromEntries(labels)[matcherKey];
  if (labelForMatcher) {
    labelValue = labelForMatcher;
  }

  const matchFunction = OperatorFunctions[operator];
  if (!matchFunction) {
    throw new Error(`no such operator: ${operator}`);
  }

  return matchFunction(labelValue, matcherValue);
}

// ⚠️ DO NOT USE THIS FUNCTION FOR ROUTE SELECTION ALGORITHM
// for route selection algorithm, always compare a single matcher to the entire label set
// see "matchLabelsSet"
function isLabelMatch(matcher: ObjectMatcher, label: Label): boolean {
  let [labelKey, labelValue] = label;
  const [matcherKey, operator, matcherValue] = matcher;

  if (labelKey !== matcherKey) {
    return false;
  }

  const matchFunction = OperatorFunctions[operator];
  if (!matchFunction) {
    throw new Error(`no such operator: ${operator}`);
  }

  return matchFunction(labelValue, matcherValue);
}

export { findMatchingAlertGroups, findMatchingRoutes, getInheritedProperties, isLabelMatchInSet };
