# Releasing AgentRecall

AgentRecall is published automatically when a semantic-version Git tag is pushed.

## One-time configuration

1. Create the GitHub Environment `vscode-marketplace`.
2. Configure its deployment tag rule as `v*`.
3. In the Microsoft Entra app registration, open **Certificates & secrets** →
   **Federated credentials** → **Add credential**, select the GitHub Actions
   scenario, and configure:
   - Organization: `clingfei`
   - Repository: `AgentRecall`
   - Entity type: `Environment`
   - Environment: `vscode-marketplace`
   - Audience: `api://AzureADTokenExchange`
4. Add these secrets to the GitHub Environment (not repository variables):
   - `AZURE_CLIENT_ID`: the app registration's **Application (client) ID**
   - `AZURE_TENANT_ID`: the app registration's **Directory (tenant) ID**
5. Commit the workflow and run the release once. The `Resolve Azure DevOps
   profile ID` step prints the application's Azure DevOps profile ID as a
   workflow notice. The following authorization check is expected to fail if
   the identity is not a Marketplace publisher member yet.
6. Open <https://marketplace.visualstudio.com/manage/publishers/clingfei>, add
   the printed profile ID as a member of publisher `clingfei`, and assign the
   **Contributor** role.
7. Rerun the failed `publish-marketplace` job. It verifies publisher access
   before uploading the VSIX files.

The expected immutable OIDC subject for this repository is:

```text
repo:clingfei@53817093/AgentRecall@1354365201:environment:vscode-marketplace
```

The numeric values are GitHub's immutable owner and repository IDs. In the
federated credential, select **Environment**, not **Tag**. The expected issuer is
`https://token.actions.githubusercontent.com`. No client secret and no Azure
subscription ID are used by this workflow.

## Publish a release

Update the version in both `package.json` and `Cargo.toml`, commit the changes, and then create a matching tag:

```bash
git tag -a v0.1.0 -m "AgentRecall v0.1.0"
git push origin main
git push origin v0.1.0
```

The release workflow rejects tags that do not exactly match the versions in both manifests. It tests the Rust core, creates platform-specific VSIX files, publishes those exact files to the VS Code Marketplace, and then attaches them and their checksums to a GitHub release.
