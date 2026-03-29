import * as cdk from "aws-cdk-lib";
import * as s3 from "aws-cdk-lib/aws-s3";
import * as cognito from "aws-cdk-lib/aws-cognito";
import * as lambda from "aws-cdk-lib/aws-lambda";
import * as apigwv2 from "aws-cdk-lib/aws-apigatewayv2";
import * as apigwv2integrations from "aws-cdk-lib/aws-apigatewayv2-integrations";
import * as apigwv2authorizers from "aws-cdk-lib/aws-apigatewayv2-authorizers";
import * as iam from "aws-cdk-lib/aws-iam";
import * as s3vectors from "aws-cdk-lib/aws-s3vectors";
import * as bedrock from "aws-cdk-lib/aws-bedrock";
import * as dynamodb from "aws-cdk-lib/aws-dynamodb";
import * as sqs from "aws-cdk-lib/aws-sqs";
import * as s3n from "aws-cdk-lib/aws-s3-notifications";
import * as lambdaEventSources from "aws-cdk-lib/aws-lambda-event-sources";
import { Construct } from "constructs";
import * as path from "path";

export class ClaudeMemoryStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props?: cdk.StackProps) {
    super(scope, id, props);

    // ========== S3 Bucket (raw transcripts) ==========
    const rawBucket = new s3.Bucket(this, "RawTranscriptBucket", {
      bucketName: `claude-memory-raw-${this.account}`,
      removalPolicy: cdk.RemovalPolicy.DESTROY,
      autoDeleteObjects: true,
      lifecycleRules: [
        {
          transitions: [
            {
              storageClass: s3.StorageClass.INFREQUENT_ACCESS,
              transitionAfter: cdk.Duration.days(90),
            },
          ],
        },
      ],
    });

    // ========== S3 Bucket (parsed for KB) ==========
    const parsedBucket = new s3.Bucket(this, "ParsedTranscriptBucket", {
      bucketName: `claude-memory-parsed-${this.account}`,
      removalPolicy: cdk.RemovalPolicy.DESTROY,
      autoDeleteObjects: true,
    });

    // ========== SQS Queue (raw → parser) ==========
    const dlq = new sqs.Queue(this, "ParserDLQ", {
      queueName: "claude-memory-parser-dlq",
      retentionPeriod: cdk.Duration.days(14),
    });

    const parserQueue = new sqs.Queue(this, "ParserQueue", {
      queueName: "claude-memory-parser-queue",
      visibilityTimeout: cdk.Duration.seconds(300),
      deadLetterQueue: { queue: dlq, maxReceiveCount: 3 },
    });

    rawBucket.addEventNotification(
      s3.EventType.OBJECT_CREATED,
      new s3n.SqsDestination(parserQueue),
      { suffix: ".jsonl" },
    );

    // ========== Parser Lambda ==========
    const parserFn = new lambda.Function(this, "ParserFn", {
      runtime: lambda.Runtime.PROVIDED_AL2023,
      architecture: lambda.Architecture.ARM_64,
      handler: "bootstrap",
      code: lambda.Code.fromAsset(
        path.join(__dirname, "../../target/lambda/parser"),
      ),
      environment: {
        PARSED_BUCKET: parsedBucket.bucketName,
        RUST_LOG: "info",
      },
      timeout: cdk.Duration.seconds(120),
      memorySize: 512,
    });

    rawBucket.grantRead(parserFn);
    parsedBucket.grantWrite(parserFn);

    parserFn.addEventSource(
      new lambdaEventSources.SqsEventSource(parserQueue, {
        batchSize: 5,
        maxConcurrency: 10,
      }),
    );

    // ========== Cognito User Pool ==========
    const userPool = new cognito.UserPool(this, "MemoryUserPool", {
      userPoolName: "claude-memory-cloud-pool",
      selfSignUpEnabled: true,
      signInAliases: { email: true },
      autoVerify: { email: true },
      passwordPolicy: {
        minLength: 8,
        requireLowercase: true,
        requireUppercase: true,
        requireDigits: true,
        requireSymbols: false,
      },
      removalPolicy: cdk.RemovalPolicy.DESTROY,
    });

    const cognitoDomain = userPool.addDomain("MemoryCognitoDomain", {
      cognitoDomain: {
        domainPrefix: `claude-memory-auth-${this.account}`,
      },
      managedLoginVersion: cognito.ManagedLoginVersion.NEWER_MANAGED_LOGIN,
    });

    // ========== API Gateway HTTP API ==========
    const httpApi = new apigwv2.HttpApi(this, "MemoryHttpApi", {
      apiName: "claude-memory-cloud-api",
      corsPreflight: {
        allowOrigins: ["*"],
        allowMethods: [apigwv2.CorsHttpMethod.ANY],
        allowHeaders: ["*"],
      },
    });

    // ========== Cognito App Client (PKCE) ==========
    const pkceClient = userPool.addClient("PkceClient", {
      userPoolClientName: "memory-cloud-cli",
      generateSecret: false,
      oAuth: {
        flows: { authorizationCodeGrant: true },
        scopes: [
          cognito.OAuthScope.OPENID,
          cognito.OAuthScope.EMAIL,
          cognito.OAuthScope.PROFILE,
        ],
        callbackUrls: ["http://localhost:8976/callback"],
        logoutUrls: ["http://localhost:8976"],
      },
      supportedIdentityProviders: [
        cognito.UserPoolClientIdentityProvider.COGNITO,
      ],
    });

    // ========== Managed Login Branding ==========
    new cognito.CfnManagedLoginBranding(this, "PkceManagedLoginBranding", {
      userPoolId: userPool.userPoolId,
      clientId: pkceClient.userPoolClientId,
      useCognitoProvidedValues: true,
    });

    // ========== S3 Vector Bucket + Index ==========
    const vectorBucket = new s3vectors.CfnVectorBucket(this, "VectorBucket", {
      vectorBucketName: `memory-cloud-vectors-${this.account}`,
    });

    const vectorIndex = new s3vectors.CfnIndex(this, "VectorIndex", {
      vectorBucketName: vectorBucket.vectorBucketName!,
      indexName: "transcript-embeddings-v2",
      dataType: "float32",
      dimension: 1024,
      distanceMetric: "cosine",
      metadataConfiguration: {
        nonFilterableMetadataKeys: [
          "AMAZON_BEDROCK_TEXT",
          "AMAZON_BEDROCK_METADATA",
        ],
      },
    });
    vectorIndex.addDependency(vectorBucket);

    // ========== Bedrock Knowledge Base ==========
    const kbRole = new iam.Role(this, "KbRole", {
      assumedBy: new iam.ServicePrincipal("bedrock.amazonaws.com"),
      inlinePolicies: {
        s3Read: new iam.PolicyDocument({
          statements: [
            new iam.PolicyStatement({
              actions: ["s3:GetObject", "s3:ListBucket"],
              resources: [
                parsedBucket.bucketArn,
                `${parsedBucket.bucketArn}/*`,
              ],
            }),
          ],
        }),
        s3Vectors: new iam.PolicyDocument({
          statements: [
            new iam.PolicyStatement({
              actions: ["s3vectors:*"],
              resources: [
                vectorBucket.attrVectorBucketArn,
                `${vectorBucket.attrVectorBucketArn}/*`,
              ],
            }),
          ],
        }),
        bedrockInvoke: new iam.PolicyDocument({
          statements: [
            new iam.PolicyStatement({
              actions: ["bedrock:InvokeModel"],
              resources: [
                `arn:aws:bedrock:${this.region}::foundation-model/amazon.titan-embed-text-v2:0`,
              ],
            }),
          ],
        }),
      },
    });

    const kb = new bedrock.CfnKnowledgeBase(this, "MemoryKb", {
      name: "memory-cloud-kb-v2",
      roleArn: kbRole.roleArn,
      knowledgeBaseConfiguration: {
        type: "VECTOR",
        vectorKnowledgeBaseConfiguration: {
          embeddingModelArn: `arn:aws:bedrock:${this.region}::foundation-model/amazon.titan-embed-text-v2:0`,
        },
      },
      storageConfiguration: {
        type: "S3_VECTORS",
        s3VectorsConfiguration: {
          vectorBucketArn: vectorBucket.attrVectorBucketArn,
          indexArn: vectorIndex.attrIndexArn,
        },
      },
    });
    kb.node.addDependency(kbRole);

    // KB DataSource now points to parsed bucket
    const dataSource = new bedrock.CfnDataSource(this, "TranscriptDataSource", {
      knowledgeBaseId: kb.attrKnowledgeBaseId,
      name: "transcript-s3",
      dataSourceConfiguration: {
        type: "S3",
        s3Configuration: { bucketArn: parsedBucket.bucketArn },
      },
      vectorIngestionConfiguration: {
        chunkingConfiguration: {
          chunkingStrategy: "SEMANTIC",
          semanticChunkingConfiguration: {
            maxTokens: 512,
            bufferSize: 0,
            breakpointPercentileThreshold: 90,
          },
        },
      },
    });

    // ========== KB Sync Lambda (parsed S3 → start ingestion) ==========
    const syncFn = new lambda.Function(this, "KbSyncFn", {
      runtime: lambda.Runtime.NODEJS_22_X,
      architecture: lambda.Architecture.ARM_64,
      handler: "index.handler",
      code: lambda.Code.fromInline(`
const { BedrockAgentClient, StartIngestionJobCommand, ListIngestionJobsCommand } = require("@aws-sdk/client-bedrock-agent");
const client = new BedrockAgentClient();
exports.handler = async (event) => {
  // Skip if an ingestion job is already running
  const list = await client.send(new ListIngestionJobsCommand({
    knowledgeBaseId: process.env.KB_ID,
    dataSourceId: process.env.DATA_SOURCE_ID,
    maxResults: 1,
    sortBy: { attribute: "STARTED_AT", order: "DESCENDING" },
  }));
  const latest = list.ingestionJobSummaries?.[0];
  if (latest && (latest.status === "STARTING" || latest.status === "IN_PROGRESS")) {
    console.log("ingestion already running:", latest.ingestionJobId);
    return;
  }
  try {
    const res = await client.send(new StartIngestionJobCommand({
      knowledgeBaseId: process.env.KB_ID,
      dataSourceId: process.env.DATA_SOURCE_ID,
    }));
    console.log("ingestion started:", res.ingestionJob?.ingestionJobId);
  } catch (e) {
    if (e.name === "ConflictException") {
      console.log("ingestion already in progress, skipping");
    } else {
      throw e;
    }
  }
};
      `),
      environment: {
        KB_ID: kb.attrKnowledgeBaseId,
        DATA_SOURCE_ID: dataSource.attrDataSourceId,
      },
      timeout: cdk.Duration.seconds(10),
      memorySize: 128,
    });

    syncFn.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ["bedrock:StartIngestionJob", "bedrock:ListIngestionJobs"],
        resources: [kb.attrKnowledgeBaseArn],
      }),
    );

    parsedBucket.addEventNotification(
      s3.EventType.OBJECT_CREATED,
      new s3n.LambdaDestination(syncFn),
      { suffix: ".md" },
    );

    // ========== DynamoDB (shares) ==========
    const sharesTable = new dynamodb.Table(this, "SharesTable", {
      tableName: "memory-cloud-shares",
      partitionKey: { name: "pk", type: dynamodb.AttributeType.STRING },
      sortKey: { name: "sk", type: dynamodb.AttributeType.STRING },
      billingMode: dynamodb.BillingMode.PAY_PER_REQUEST,
      removalPolicy: cdk.RemovalPolicy.DESTROY,
    });

    sharesTable.addGlobalSecondaryIndex({
      indexName: "ByOwner",
      partitionKey: { name: "owner_id", type: dynamodb.AttributeType.STRING },
      sortKey: { name: "sk", type: dynamodb.AttributeType.STRING },
    });

    // ========== API Lambda ==========
    const apiFn = new lambda.Function(this, "ApiFn", {
      runtime: lambda.Runtime.PROVIDED_AL2023,
      architecture: lambda.Architecture.ARM_64,
      handler: "bootstrap",
      code: lambda.Code.fromAsset(
        path.join(__dirname, "../../target/lambda/api"),
      ),
      environment: {
        COGNITO_DOMAIN: `${cognitoDomain.domainName}.auth.${this.region}.amazoncognito.com`,
        COGNITO_CLIENT_ID: pkceClient.userPoolClientId,
        COGNITO_USER_POOL_ID: userPool.userPoolId,
        TRANSCRIPT_BUCKET: rawBucket.bucketName,
        KB_ID: kb.attrKnowledgeBaseId,
        SHARES_TABLE: sharesTable.tableName,
        RUST_LOG: "info",
      },
      timeout: cdk.Duration.seconds(30),
      memorySize: 256,
    });

    rawBucket.grantReadWrite(apiFn);
    sharesTable.grantReadWriteData(apiFn);
    apiFn.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ["bedrock:Retrieve"],
        resources: [kb.attrKnowledgeBaseArn],
      }),
    );
    apiFn.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ["cognito-idp:AdminGetUser"],
        resources: [userPool.userPoolArn],
      }),
    );

    // ========== Cognito Authorizer ==========
    const authorizer = new apigwv2authorizers.HttpUserPoolAuthorizer(
      "CognitoAuthorizer",
      userPool,
      { userPoolClients: [pkceClient] },
    );

    // ========== API Gateway Integrations & Routes ==========
    const apiIntegration = new apigwv2integrations.HttpLambdaIntegration(
      "ApiIntegration",
      apiFn,
    );

    // /config — unauthenticated
    httpApi.addRoutes({
      path: "/config",
      methods: [apigwv2.HttpMethod.GET],
      integration: apiIntegration,
    });

    // /transcript — authenticated
    httpApi.addRoutes({
      path: "/transcript",
      methods: [apigwv2.HttpMethod.POST],
      integration: apiIntegration,
      authorizer,
    });
    httpApi.addRoutes({
      path: "/transcript/{proxy+}",
      methods: [apigwv2.HttpMethod.GET, apigwv2.HttpMethod.DELETE],
      integration: apiIntegration,
      authorizer,
    });

    // /sessions — authenticated
    httpApi.addRoutes({
      path: "/sessions",
      methods: [apigwv2.HttpMethod.GET],
      integration: apiIntegration,
      authorizer,
    });

    // /recall — authenticated
    httpApi.addRoutes({
      path: "/recall",
      methods: [apigwv2.HttpMethod.POST],
      integration: apiIntegration,
      authorizer,
    });

    // /shares — authenticated
    httpApi.addRoutes({
      path: "/shares",
      methods: [apigwv2.HttpMethod.GET, apigwv2.HttpMethod.POST],
      integration: apiIntegration,
      authorizer,
    });
    httpApi.addRoutes({
      path: "/shares/{owner_id}",
      methods: [apigwv2.HttpMethod.DELETE],
      integration: apiIntegration,
      authorizer,
    });
    httpApi.addRoutes({
      path: "/shares/recipients/{recipient_id}",
      methods: [apigwv2.HttpMethod.DELETE],
      integration: apiIntegration,
      authorizer,
    });

    // ========== Outputs ==========
    new cdk.CfnOutput(this, "ApiUrl", {
      value: httpApi.apiEndpoint,
    });
    new cdk.CfnOutput(this, "UserPoolId", {
      value: userPool.userPoolId,
    });
    new cdk.CfnOutput(this, "CognitoDomain", {
      value: `https://${cognitoDomain.domainName}.auth.${this.region}.amazoncognito.com`,
    });
    new cdk.CfnOutput(this, "ClientId", {
      value: pkceClient.userPoolClientId,
    });
    new cdk.CfnOutput(this, "KnowledgeBaseId", {
      value: kb.attrKnowledgeBaseId,
    });
    new cdk.CfnOutput(this, "RawBucketName", {
      value: rawBucket.bucketName,
    });
    new cdk.CfnOutput(this, "ParsedBucketName", {
      value: parsedBucket.bucketName,
    });
  }
}
