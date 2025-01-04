#!/usr/bin/env node
import { Command } from 'commander';
import { build, watch } from './commands';
import chalk from 'chalk';

const program = new Command();

/**
 * Creates a runner function for a command, that handles errors and exits the process with a non-zero exit code.
 * @param fn - The function to run.
 * @returns The runner function.
 */
function runner(fn: (...args: any[]) => Promise<void>) {
    return async (...args: any[]) => {
        try {
            await fn(...args);
        } catch (error) {
            const buildError = error instanceof Error ? error : new Error('Unknown error occurred');
            console.error(chalk.red(`\n${buildError.message}`));
            console.error(chalk.red(`${buildError.stack}`));
            process.exit(1);
        }
    }
}

program
  .name('caido-dev')
  .description('Development tools for building Caido plugins')
  .version('1.0.0');

program
  .command('build [path]')
  .description('Build the Caido plugin')
  .option('-c, --config <path>', 'Path to caido.config.ts file')
  .action(runner((path, args) => build({ path, ...args })));

program
  .command('watch [path]')
  .description('Start development server and watch for changes')
  .option('-c, --config <path>', 'Path to caido.config.ts file')
  .action(runner((path, args) => watch({ path, ...args })));

program
  .command('dev')
  .description('Start development server')
  .option('-c, --config <path>', 'Path to caido.config.ts file')
  .action(() => {});

try {
  program.parse(); 
} catch (error) {
    const buildError = error instanceof Error ? error : new Error('Unknown error occurred');
    console.error(chalk.red(`\n${buildError.message}`));
    process.exit(1);
}