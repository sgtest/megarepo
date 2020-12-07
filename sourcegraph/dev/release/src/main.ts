import { loadConfig } from './config'
import { runStep, StepID } from './release'

/**
 * Release captain automation
 */
async function main(): Promise<void> {
    const config = loadConfig()
    const args = process.argv.slice(2)
    if (args.length === 0) {
        await runStep(config, 'help')
        console.error('The release tool expects at least 1 argument')
        return
    }

    const step = args[0] as StepID
    const stepArguments = args.slice(1)
    await runStep(config, step, ...stepArguments)
}

main().catch(error => console.error(error))
