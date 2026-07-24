"""Generated from the private Cerul OpenAPI. Do not edit."""

CONTRACT_SHA256 = "b3fca22458a20b7b37107ac1ff473ad0600ef586db609518fa8518da2100d32a"
SOURCE_SHA256 = "62d9256445051b45d3f0039cefa0fa59de20c64050e97596b674da56da232df2"
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
    "getPublicShare": {
        "method": "GET",
        "path": "/v1/shares/{share_id}",
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
PUBLIC_SCHEMA_NAMES = ("AddLibraryItemRequest", "AgentSession", "AgentSessionId", "AgentSessionResponse", "ApiCredential", "ApiCredentialId", "ApiCredentialListResponse", "Artifact", "ArtifactId", "ArtifactListResponse", "ArtifactResponse", "Asset", "AssetId", "AssetResponse", "AuthorizationCodeTokenRequest", "Capability", "CapabilityListResponse", "CreateApiCredentialRequest", "CreateJobRequest", "CreateLibraryRequest", "CreateResponseRequest", "CreateReviewEventRequest", "CreateUploadRequest", "CreatedApiCredential", "CreatedApiCredentialResponse", "CreditReservationSummary", "DeviceAuthorizationRequest", "DeviceAuthorizationResponse", "DeviceCodeTokenRequest", "EntitledCapability", "EntitlementLimit", "ErrorDetail", "ErrorEnvelope", "Evidence", "EvidenceId", "EvidenceLocator", "Execution", "ExecutionPolicy", "IndexRequest", "IngestionProfile", "Job", "JobId", "JobResponse", "Library", "LibraryId", "LibraryItem", "LibraryItemId", "LibraryItemResponse", "LibraryListResponse", "LibraryResponse", "OAuthScopeList", "OAuthTokenRequest", "OAuthTokenResponse", "Principal", "PrincipalResponse", "PublicShare", "PublicShareResponse", "RefreshTokenRequest", "RequestId", "Response", "ResponseEnvelope", "ResponseEvent", "ResponseId", "ResponseInputMessage", "ResponseMetadata", "ResponseOutputItem", "ReviewEvent", "ReviewEventResponse", "RuntimeKind", "RuntimeLocation", "Scope", "SearchRequest", "SearchResponse", "SearchResult", "ShareId", "StructuredTextFormat", "UploadResponse", "UploadSession", "Usage", "UsageMeterTotal", "UsageReport", "UsageReportResponse", "UserId", "WorkflowReference", "Workspace", "WorkspaceEntitlement", "WorkspaceEntitlementResponse", "WorkspaceId", "WorkspaceListResponse",)
