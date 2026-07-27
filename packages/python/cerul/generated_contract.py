"""Generated from the private Cerul OpenAPI. Do not edit."""

CONTRACT_SHA256 = "c15615d54993468465fc1528fdba783f7c430a290d6e72546bc7b83b7ecfa0c3"
SOURCE_SHA256 = "9ee937bb3d1325232e729e114ee783b3c461bf2581ba79fce26b652248972852"
PUBLIC_OPERATIONS = {
    "createDeviceAuthorization": {
        "method": "POST",
        "path": "/v1/oauth/device/authorize",
    },
    "exchangeOAuthToken": {
        "method": "POST",
        "path": "/v1/oauth/token",
    },
    "listWorkspaces": {
        "method": "GET",
        "path": "/v1/workspaces",
    },
    "getUsageReport": {
        "method": "GET",
        "path": "/v1/usage",
    },
    "getPrincipal": {
        "method": "GET",
        "path": "/v1/principal",
    },
    "getWorkspaceEntitlement": {
        "method": "GET",
        "path": "/v1/workspaces/{workspace_id}/entitlement",
    },
    "listApiCredentials": {
        "method": "GET",
        "path": "/v1/workspaces/{workspace_id}/api-credentials",
    },
    "createApiCredential": {
        "method": "POST",
        "path": "/v1/workspaces/{workspace_id}/api-credentials",
    },
    "revokeApiCredential": {
        "method": "DELETE",
        "path": "/v1/workspaces/{workspace_id}/api-credentials/{api_credential_id}",
    },
    "listCapabilities": {
        "method": "GET",
        "path": "/v1/capabilities",
    },
    "listLibraries": {
        "method": "GET",
        "path": "/v1/libraries",
    },
    "createLibrary": {
        "method": "POST",
        "path": "/v1/libraries",
    },
    "addLibraryItem": {
        "method": "POST",
        "path": "/v1/libraries/{library_id}/items",
    },
    "getLibraryItem": {
        "method": "GET",
        "path": "/v1/library-items/{library_item_id}",
    },
    "createUpload": {
        "method": "POST",
        "path": "/v1/uploads",
    },
    "indexAsset": {
        "method": "POST",
        "path": "/v1/index",
    },
    "getAsset": {
        "method": "GET",
        "path": "/v1/assets/{asset_id}",
    },
    "getEvidenceFrame": {
        "method": "GET",
        "path": "/v1/evidence/{evidence_id}/frame",
    },
    "getEvidenceVideoSegment": {
        "method": "GET",
        "path": "/v1/evidence/{evidence_id}/video-segment",
    },
    "getEvidenceVideoClip": {
        "method": "GET",
        "path": "/v1/evidence/{evidence_id}/video-clip",
    },
    "searchEvidence": {
        "method": "POST",
        "path": "/v1/search",
    },
    "createJob": {
        "method": "POST",
        "path": "/v1/jobs",
    },
    "getJob": {
        "method": "GET",
        "path": "/v1/jobs/{job_id}",
    },
    "cancelJob": {
        "method": "POST",
        "path": "/v1/jobs/{job_id}/cancel",
    },
    "listJobArtifacts": {
        "method": "GET",
        "path": "/v1/jobs/{job_id}/artifacts",
    },
    "getArtifact": {
        "method": "GET",
        "path": "/v1/artifacts/{artifact_id}",
    },
    "createArtifactReview": {
        "method": "POST",
        "path": "/v1/artifacts/{artifact_id}/reviews",
    },
    "createResponse": {
        "method": "POST",
        "path": "/v1/responses",
    },
    "getAgentSession": {
        "method": "GET",
        "path": "/v1/agent-sessions/{agent_session_id}",
    },
    "deleteAgentSession": {
        "method": "DELETE",
        "path": "/v1/agent-sessions/{agent_session_id}",
    },
}
PUBLIC_SCHEMA_NAMES = ("AddLibraryItemRequest", "AgentSession", "AgentSessionId", "AgentSessionResponse", "ApiCredential", "ApiCredentialId", "ApiCredentialListResponse", "Artifact", "ArtifactId", "ArtifactListResponse", "ArtifactResponse", "Asset", "AssetId", "AssetResponse", "AuthorizationCodeTokenRequest", "Capability", "CapabilityListResponse", "CreateApiCredentialRequest", "CreateJobRequest", "CreateLibraryRequest", "CreateResponseRequest", "CreateReviewEventRequest", "CreateUploadRequest", "CreatedApiCredential", "CreatedApiCredentialResponse", "CreditReservationSummary", "DeviceAuthorizationRequest", "DeviceAuthorizationResponse", "DeviceCodeTokenRequest", "EntitledCapability", "EntitlementLimit", "ErrorDetail", "ErrorEnvelope", "Evidence", "EvidenceId", "EvidenceLocator", "Execution", "ExecutionPolicy", "IndexRequest", "IngestionProfile", "Job", "JobId", "JobResponse", "Library", "LibraryId", "LibraryItem", "LibraryItemId", "LibraryItemResponse", "LibraryListResponse", "LibraryResponse", "OAuthScopeList", "OAuthTokenRequest", "OAuthTokenResponse", "Principal", "PrincipalResponse", "RefreshTokenRequest", "RequestId", "Response", "ResponseEnvelope", "ResponseEvent", "ResponseId", "ResponseInputMessage", "ResponseMetadata", "ResponseOutputItem", "ReviewEvent", "ReviewEventResponse", "RuntimeKind", "RuntimeLocation", "Scope", "SearchRequest", "SearchResponse", "SearchResult", "StructuredTextFormat", "UploadResponse", "UploadSession", "Usage", "UsageMeterTotal", "UsageReport", "UsageReportResponse", "UserId", "WorkflowReference", "Workspace", "WorkspaceEntitlement", "WorkspaceEntitlementResponse", "WorkspaceId", "WorkspaceListResponse",)
