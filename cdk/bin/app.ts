#!/usr/bin/env node
import "source-map-support/register";
import * as cdk from "aws-cdk-lib";
import { ClaudeMemoryStack } from "../lib/claude-memory-stack";

const app = new cdk.App();
new ClaudeMemoryStack(app, "ClaudeMemoryStack", {
  env: {
    account: process.env.CDK_DEFAULT_ACCOUNT,
    region: process.env.CDK_DEFAULT_REGION,
  },
});
