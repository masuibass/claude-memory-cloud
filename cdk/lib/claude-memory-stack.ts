import * as cdk from "aws-cdk-lib";
import * as ec2 from "aws-cdk-lib/aws-ec2";
import * as rds from "aws-cdk-lib/aws-rds";
import * as s3 from "aws-cdk-lib/aws-s3";
import * as cognito from "aws-cdk-lib/aws-cognito";
import * as lambda from "aws-cdk-lib/aws-lambda";
import * as secretsmanager from "aws-cdk-lib/aws-secretsmanager";
import * as apigwv2 from "aws-cdk-lib/aws-apigatewayv2";
import * as apigwv2integrations from "aws-cdk-lib/aws-apigatewayv2-integrations";
import * as apigwv2authorizers from "aws-cdk-lib/aws-apigatewayv2-authorizers";
import * as iam from "aws-cdk-lib/aws-iam";
import { Construct } from "constructs";
import * as path from "path";

export class ClaudeMemoryStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props?: cdk.StackProps) {
    super(scope, id, props);

    // ========== VPC ==========
    const vpc = new ec2.Vpc(this, "MemoryVpc", {
      maxAzs: 2,
      natGateways: 1,
      subnetConfiguration: [
        {
          name: "Public",
          subnetType: ec2.SubnetType.PUBLIC,
          cidrMask: 24,
        },
        {
          name: "Private",
          subnetType: ec2.SubnetType.PRIVATE_WITH_EGRESS,
          cidrMask: 24,
        },
        {
          name: "Isolated",
          subnetType: ec2.SubnetType.PRIVATE_ISOLATED,
          cidrMask: 24,
        },
      ],
    });

    // ========== Aurora Security Group ==========
    const auroraSg = new ec2.SecurityGroup(this, "AuroraSg", {
      vpc,
      description: "Aurora Serverless v2 security group",
    });

    // ========== Aurora Serverless v2 (PostgreSQL) ==========
    const auroraCluster = new rds.DatabaseCluster(this, "MemoryDb", {
      engine: rds.DatabaseClusterEngine.auroraPostgres({
        version: rds.AuroraPostgresEngineVersion.VER_16_4,
      }),
      serverlessV2MinCapacity: 0,
      serverlessV2MaxCapacity: 2,
      writer: rds.ClusterInstance.serverlessV2("Writer", {
        scaleWithWriter: true,
      }),
      vpc,
      vpcSubnets: { subnetType: ec2.SubnetType.PRIVATE_ISOLATED },
      securityGroups: [auroraSg],
      defaultDatabaseName: "memory",
      removalPolicy: cdk.RemovalPolicy.DESTROY,
      enableDataApi: true,
    });

    // ========== S3 Bucket (transcripts) ==========
    const transcriptBucket = new s3.Bucket(this, "TranscriptBucket", {
      bucketName: `claude-memory-transcripts-${this.account}`,
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

    // ========== Cognito App Client ==========
    const appClient = userPool.addClient("MemoryProxyClient", {
      userPoolClientName: "memory-oauth-proxy",
      generateSecret: true,
      oAuth: {
        flows: { authorizationCodeGrant: true },
        scopes: [
          cognito.OAuthScope.OPENID,
          cognito.OAuthScope.EMAIL,
          cognito.OAuthScope.PROFILE,
        ],
        callbackUrls: [`${httpApi.apiEndpoint}/callback`],
        logoutUrls: [`${httpApi.apiEndpoint}`],
      },
      supportedIdentityProviders: [
        cognito.UserPoolClientIdentityProvider.COGNITO,
      ],
      authFlows: { userSrp: true },
    });

    // ========== Managed Login Branding ==========
    new cognito.CfnManagedLoginBranding(this, "ManagedLoginBranding", {
      userPoolId: userPool.userPoolId,
      clientId: appClient.userPoolClientId,
      useCognitoProvidedValues: true,
    });

    // ========== Secrets Manager ==========
    const oauthSecret = new secretsmanager.Secret(this, "MemoryOAuthSecret", {
      secretName: "claude-memory-cloud/oauth-secret",
      generateSecretString: {
        secretStringTemplate: JSON.stringify({
          cognitoClientId: appClient.userPoolClientId,
        }),
        generateStringKey: "serverSecret",
        excludePunctuation: true,
        passwordLength: 64,
      },
    });

    // ========== Lambda Security Group ==========
    const lambdaSg = new ec2.SecurityGroup(this, "LambdaSg", {
      vpc,
      description: "Lambda security group",
    });

    auroraSg.addIngressRule(
      lambdaSg,
      ec2.Port.tcp(5432),
      "Allow Lambda to Aurora"
    );

    // ========== Shared environment ==========
    const oauthEnv = {
      COGNITO_USER_POOL_ID: userPool.userPoolId,
      COGNITO_CLIENT_ID: appClient.userPoolClientId,
      COGNITO_DOMAIN: `${cognitoDomain.domainName}.auth.${this.region}.amazoncognito.com`,
      API_URL: httpApi.apiEndpoint,
      SECRET_ARN: oauthSecret.secretArn,
      REGION: this.region,
    };

    const dbEnv = {
      DB_SECRET_ARN: auroraCluster.secret!.secretArn,
      DB_CLUSTER_ENDPOINT: auroraCluster.clusterEndpoint.hostname,
      DB_NAME: "memory",
    };

    // ========== OAuth Metadata Lambda ==========
    const oauthMetadataFn = new lambda.Function(this, "OAuthMetadataFn", {
      runtime: lambda.Runtime.PROVIDED_AL2023,
      architecture: lambda.Architecture.ARM_64,
      handler: "bootstrap",
      code: lambda.Code.fromAsset(
        path.join(__dirname, "../../target/lambda/oauth-metadata")
      ),
      environment: {
        API_URL: httpApi.apiEndpoint,
        RUST_LOG: "info",
      },
      timeout: cdk.Duration.seconds(10),
      memorySize: 128,
    });

    // ========== OAuth Proxy Lambda ==========
    const oauthProxyFn = new lambda.Function(this, "OAuthProxyFn", {
      runtime: lambda.Runtime.PROVIDED_AL2023,
      architecture: lambda.Architecture.ARM_64,
      handler: "bootstrap",
      code: lambda.Code.fromAsset(
        path.join(__dirname, "../../target/lambda/oauth-proxy")
      ),
      environment: {
        ...oauthEnv,
        RUST_LOG: "info",
      },
      timeout: cdk.Duration.seconds(30),
      memorySize: 256,
    });

    oauthSecret.grantRead(oauthProxyFn);
    userPool.grant(oauthProxyFn, "cognito-idp:DescribeUserPoolClient");

    // ========== API Lambda (CRUD + search) ==========
    const apiFn = new lambda.Function(this, "ApiFn", {
      runtime: lambda.Runtime.PROVIDED_AL2023,
      architecture: lambda.Architecture.ARM_64,
      handler: "bootstrap",
      code: lambda.Code.fromAsset(
        path.join(__dirname, "../../target/lambda/api")
      ),
      vpc,
      vpcSubnets: { subnetType: ec2.SubnetType.PRIVATE_WITH_EGRESS },
      securityGroups: [lambdaSg],
      environment: {
        ...dbEnv,
        TRANSCRIPT_BUCKET: transcriptBucket.bucketName,
        RUST_LOG: "info",
      },
      timeout: cdk.Duration.seconds(30),
      memorySize: 512,
    });

    auroraCluster.secret!.grantRead(apiFn);
    transcriptBucket.grantReadWrite(apiFn);
    apiFn.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ["bedrock:InvokeModel"],
        resources: [
          `arn:aws:bedrock:${this.region}::foundation-model/amazon.titan-embed-text-v2:0`,
        ],
      })
    );

    // ========== MCP Server Lambda ==========
    const mcpServerFn = new lambda.Function(this, "McpServerFn", {
      runtime: lambda.Runtime.PROVIDED_AL2023,
      architecture: lambda.Architecture.ARM_64,
      handler: "bootstrap",
      code: lambda.Code.fromAsset(
        path.join(__dirname, "../../target/lambda/mcp-server")
      ),
      vpc,
      vpcSubnets: { subnetType: ec2.SubnetType.PRIVATE_WITH_EGRESS },
      securityGroups: [lambdaSg],
      environment: {
        ...dbEnv,
        TRANSCRIPT_BUCKET: transcriptBucket.bucketName,
        RUST_LOG: "info",
      },
      timeout: cdk.Duration.seconds(30),
      memorySize: 512,
    });

    auroraCluster.secret!.grantRead(mcpServerFn);
    transcriptBucket.grantRead(mcpServerFn);
    mcpServerFn.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ["bedrock:InvokeModel"],
        resources: [
          `arn:aws:bedrock:${this.region}::foundation-model/amazon.titan-embed-text-v2:0`,
        ],
      })
    );

    // ========== JWT Authorizer ==========
    const jwtAuthorizer = new apigwv2authorizers.HttpJwtAuthorizer(
      "CognitoJwtAuthorizer",
      `https://cognito-idp.${this.region}.amazonaws.com/${userPool.userPoolId}`,
      {
        jwtAudience: [appClient.userPoolClientId],
      }
    );

    // ========== API Gateway Integrations ==========
    const metadataIntegration =
      new apigwv2integrations.HttpLambdaIntegration(
        "MetadataIntegration",
        oauthMetadataFn
      );

    const proxyIntegration = new apigwv2integrations.HttpLambdaIntegration(
      "ProxyIntegration",
      oauthProxyFn
    );

    const apiIntegration = new apigwv2integrations.HttpLambdaIntegration(
      "ApiIntegration",
      apiFn
    );

    const mcpIntegration = new apigwv2integrations.HttpLambdaIntegration(
      "McpIntegration",
      mcpServerFn
    );

    // ========== API Gateway Routes ==========

    // OAuth Metadata (unauthenticated)
    httpApi.addRoutes({
      path: "/.well-known/{proxy+}",
      methods: [apigwv2.HttpMethod.GET],
      integration: metadataIntegration,
    });

    // OAuth Proxy (unauthenticated)
    httpApi.addRoutes({
      path: "/register",
      methods: [apigwv2.HttpMethod.POST],
      integration: proxyIntegration,
    });
    httpApi.addRoutes({
      path: "/authorize",
      methods: [apigwv2.HttpMethod.GET],
      integration: proxyIntegration,
    });
    httpApi.addRoutes({
      path: "/callback",
      methods: [apigwv2.HttpMethod.GET],
      integration: proxyIntegration,
    });
    httpApi.addRoutes({
      path: "/token",
      methods: [apigwv2.HttpMethod.POST],
      integration: proxyIntegration,
    });

    // API (authenticated)
    httpApi.addRoutes({
      path: "/api/{proxy+}",
      methods: [
        apigwv2.HttpMethod.GET,
        apigwv2.HttpMethod.POST,
        apigwv2.HttpMethod.PUT,
        apigwv2.HttpMethod.DELETE,
      ],
      integration: apiIntegration,
      authorizer: jwtAuthorizer,
    });

    // MCP Server (authenticated)
    httpApi.addRoutes({
      path: "/mcp",
      methods: [
        apigwv2.HttpMethod.POST,
        apigwv2.HttpMethod.GET,
        apigwv2.HttpMethod.DELETE,
      ],
      integration: mcpIntegration,
      authorizer: jwtAuthorizer,
    });

    // ========== Outputs ==========
    new cdk.CfnOutput(this, "ApiUrl", {
      value: httpApi.apiEndpoint,
      description: "API Gateway endpoint URL",
    });
    new cdk.CfnOutput(this, "UserPoolId", {
      value: userPool.userPoolId,
      description: "Cognito User Pool ID",
    });
    new cdk.CfnOutput(this, "CognitoDomain", {
      value: `https://${cognitoDomain.domainName}.auth.${this.region}.amazoncognito.com`,
      description: "Cognito Hosted UI domain",
    });
    new cdk.CfnOutput(this, "ClientId", {
      value: appClient.userPoolClientId,
      description: "Cognito App Client ID (proxy)",
    });
    new cdk.CfnOutput(this, "AuroraClusterEndpoint", {
      value: auroraCluster.clusterEndpoint.hostname,
      description: "Aurora cluster endpoint",
    });
    new cdk.CfnOutput(this, "TranscriptBucketName", {
      value: transcriptBucket.bucketName,
      description: "S3 bucket for transcripts",
    });
  }
}
