import { llms } from '@grafana/experimental';

import { DashboardModel } from '../../state';
import { Diffs, jsonDiff } from '../VersionHistory/utils';

export interface Message {
  role: Role;
  content: string;
}

export enum Role {
  // System content cannot be overwritten by user propmts.
  'system' = 'system',
  // User content is the content that the user has entered.
  // This content can be overwritten by following propmt.
  'user' = 'user',
}

// TODO: Replace this approach with more stable approach
export const SPECIAL_DONE_TOKEN = '¬';

/**
 * The llm library doesn't indicate when the stream is done, so we need to ask the LLM to add an special token to indicate that the stream is done at the end of the message.
 */
export const DONE_MESSAGE = {
  role: Role.system,
  content: `When you are done with the response, write "${SPECIAL_DONE_TOKEN}" always at the end of the response.`,
};

/**
 * The OpenAI model to be used.
 */
export const OPEN_AI_MODEL = 'gpt-4';

/**
 * Generate a text with the instructions for LLM to follow.
 * Every message will be sent to LLM as a prompt. The messages will be sent in order. The messages will be composed by the content and the role.
 *
 * The role can be system or user.
 * - System messages cannot be overwritten by user input. They are used to send instructions to LLM about how to behave or how to format the response.
 * - User messages can be overwritten by user input and they will be used to send manually user input.
 *
 * @param messages messages to send to LLM
 * @param onReply callback to call when LLM replies. The reply will be streamed, so it will be called for every token received.
 * @param temperature what temperature to use when calling the llm. default 1.
 * @returns The subscription to the stream.
 */
export const generateTextWithLLM = async (
  messages: Message[],
  onReply: (response: string, isDone: boolean) => void,
  temperature = 1
) => {
  const enabled = await isLLMPluginEnabled();

  if (!enabled) {
    throw Error('LLM plugin is not enabled');
  }

  return llms.openai
    .streamChatCompletions({
      model: OPEN_AI_MODEL,
      messages: [DONE_MESSAGE, ...messages],
      temperature,
    })
    .pipe(
      // Accumulate the stream content into a stream of strings, where each
      // element contains the accumulated message so far.
      llms.openai.accumulateContent()
    )
    .subscribe((response) => {
      return onReply(cleanupResponse(response), isResponseCompleted(response));
    });
};

/**
 * Check if the LLM plugin is enabled and configured.
 * @returns true if the LLM plugin is enabled and configured.
 */
export async function isLLMPluginEnabled() {
  // Check if the LLM plugin is enabled and configured.
  // If not, we won't be able to make requests, so return early.
  return await llms.openai.enabled();
}

/**
 * Check if the response is completed using the special done token.
 * @param response The response to check.
 * @returns true if the response is completed.
 */
export function isResponseCompleted(response: string) {
  return response.endsWith(SPECIAL_DONE_TOKEN);
}

/**
 * Remove the special done token and quotes from the response.
 * @param response The response to clean up.
 * @returns The cleaned up response.
 */
export function cleanupResponse(response: string) {
  return response.replace(SPECIAL_DONE_TOKEN, '').replace(/"/g, '');
}

/**
 * Diff the current dashboard with the original dashboard and the dashboard after migration
 * to split the changes into user changes and migration changes.
 * * User changes: changes made by the user
 * * Migration changes: changes made by the DashboardMigrator after opening the dashboard
 *
 * @param dashboard current dashboard to be saved
 * @returns user changes and migration changes
 */
export function getDashboardChanges(dashboard: DashboardModel): {
  userChanges: Diffs;
  migrationChanges: Diffs;
} {
  // Re-parse the dashboard to remove functions and other non-serializable properties
  const currentDashboard = JSON.parse(JSON.stringify(dashboard.getSaveModelClone()));
  const originalDashboard = dashboard.getOriginalDashboard()!;
  const dashboardAfterMigration = JSON.parse(JSON.stringify(new DashboardModel(originalDashboard).getSaveModelClone()));

  return {
    userChanges: jsonDiff(dashboardAfterMigration, currentDashboard),
    migrationChanges: jsonDiff(originalDashboard, dashboardAfterMigration),
  };
}
