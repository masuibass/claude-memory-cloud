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
import { Construct } from "constructs";
import * as path from "path";

export class ClaudeMemoryStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props?: cdk.StackProps) {
    super(scope, id, props);

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
                transcriptBucket.bucketArn,
                `${transcriptBucket.bucketArn}/*`,
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

    new bedrock.CfnDataSource(this, "TranscriptDataSource", {
      knowledgeBaseId: kb.attrKnowledgeBaseId,
      name: "transcript-s3",
      dataSourceConfiguration: {
        type: "S3",
        s3Configuration: { bucketArn: transcriptBucket.bucketArn },
      },
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
        TRANSCRIPT_BUCKET: transcriptBucket.bucketName,
        KB_ID: kb.attrKnowledgeBaseId,
        RUST_LOG: "info",
      },
      timeout: cdk.Duration.seconds(30),
      memorySize: 256,
    });

    transcriptBucket.grantReadWrite(apiFn);
    apiFn.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ["bedrock:Retrieve"],
        resources: [kb.attrKnowledgeBaseArn],
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
      path: "/transcript/{user_id}/{sid}",
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
    new cdk.CfnOutput(this, "TranscriptBucketName", {
      value: transcriptBucket.bucketName,
    });
  }
}
