import * as vscode from "vscode";
import * as toolchain from "./toolchain";
import type { Config } from "./config";
import { log } from "./util";

// This ends up as the `type` key in tasks.json. RLS also uses `cargo` and
// our configuration should be compatible with it so use the same key.
export const TASK_TYPE = "cargo";
export const TASK_SOURCE = "rust";

export interface RustTargetDefinition extends vscode.TaskDefinition {
    program: string;
    args: string[];
    cwd?: string;
    env?: { [key: string]: string };
}

class RustTaskProvider implements vscode.TaskProvider {
    private readonly config: Config;

    constructor(config: Config) {
        this.config = config;
    }

    async provideTasks(): Promise<vscode.Task[]> {
        // Detect Rust tasks. Currently we do not do any actual detection
        // of tasks (e.g. aliases in .cargo/config) and just return a fixed
        // set of tasks that always exist. These tasks cannot be removed in
        // tasks.json - only tweaked.

        const defs = [
            { command: "build", group: vscode.TaskGroup.Build },
            { command: "check", group: vscode.TaskGroup.Build },
            { command: "clippy", group: vscode.TaskGroup.Build },
            { command: "test", group: vscode.TaskGroup.Test },
            { command: "clean", group: vscode.TaskGroup.Clean },
            { command: "run", group: undefined },
        ];

        const cargoPath = await toolchain.cargoPath();

        const tasks: vscode.Task[] = [];
        for (const workspaceTarget of vscode.workspace.workspaceFolders || []) {
            for (const def of defs) {
                const vscodeTask = await buildRustTask(
                    workspaceTarget,
                    { type: TASK_TYPE, program: cargoPath, args: [def.command] },
                    `cargo ${def.command}`,
                    this.config.problemMatcher,
                    this.config.cargoRunner,
                );
                vscodeTask.group = def.group;
                tasks.push(vscodeTask);
            }
        }

        return tasks;
    }

    async resolveTask(task: vscode.Task): Promise<vscode.Task | undefined> {
        // VSCode calls this for every cargo task in the user's tasks.json,
        // we need to inform VSCode how to execute that command by creating
        // a ShellExecution for it.

        const definition = task.definition as RustTargetDefinition;

        if (definition.type === TASK_TYPE) {
            return await buildRustTask(
                task.scope,
                definition,
                task.name,
                this.config.problemMatcher,
                this.config.cargoRunner,
            );
        }

        return undefined;
    }
}

export async function buildRustTask(
    scope: vscode.WorkspaceFolder | vscode.TaskScope | undefined,
    definition: RustTargetDefinition,
    name: string,
    problemMatcher: string[],
    customRunner?: string,
    throwOnError: boolean = false,
): Promise<vscode.Task> {
    let exec: vscode.ProcessExecution | vscode.ShellExecution | undefined = undefined;

    if (customRunner) {
        const runnerCommand = `${customRunner}.buildShellExecution`;
        try {
            const runnerArgs = {
                kind: TASK_TYPE,
                args: definition.args,
                cwd: definition.cwd,
                env: definition.env,
            };
            const customExec = await vscode.commands.executeCommand(runnerCommand, runnerArgs);
            if (customExec) {
                if (customExec instanceof vscode.ShellExecution) {
                    exec = customExec;
                } else {
                    log.debug("Invalid cargo ShellExecution", customExec);
                    throw "Invalid cargo ShellExecution.";
                }
            }
            // fallback to default processing
        } catch (e) {
            if (throwOnError) throw `Cargo runner '${customRunner}' failed! ${e}`;
            // fallback to default processing
        }
    }

    if (!exec) {
        exec = new vscode.ProcessExecution(definition.program, definition.args, definition);
    }

    return new vscode.Task(
        definition,
        // scope can sometimes be undefined. in these situations we default to the workspace taskscope as
        // recommended by the official docs: https://code.visualstudio.com/api/extension-guides/task-provider#task-provider)
        scope ?? vscode.TaskScope.Workspace,
        name,
        TASK_SOURCE,
        exec,
        problemMatcher,
    );
}

export function activateTaskProvider(config: Config): vscode.Disposable {
    const provider = new RustTaskProvider(config);
    return vscode.tasks.registerTaskProvider(TASK_TYPE, provider);
}
