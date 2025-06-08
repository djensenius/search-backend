# GitHub Pages Setup

This document describes the GitHub Pages setup for the Search Engine Backend project.

## Overview

This repository is configured with GitHub Pages to automatically deploy Rust documentation. The setup includes:

- **gh-pages branch**: Dedicated branch for GitHub Pages content
- **Automated workflow**: Deploys documentation on main branch changes
- **Landing page**: Navigation to documentation and project resources

## Branch Structure

### Main Branch
Contains the source code and workflow configuration:
- `.github/workflows/rustdoc.yml` - Documentation generation and deployment workflow

### gh-pages Branch  
Contains the GitHub Pages site content:
- `index.html` - Landing page with navigation
- `doc/` - Auto-generated Rust documentation (populated by workflow)
- `README.md` - Branch-specific documentation
- `.gitignore` - Pages-specific ignore patterns

## Workflow Process

When changes are pushed to the main branch:

1. **Generate Documentation**
   ```bash
   cargo doc --no-deps --all-features --document-private-items
   ```

2. **Deploy to gh-pages**
   - Creates or updates the gh-pages branch
   - Copies generated documentation
   - Commits and pushes changes

3. **GitHub Pages Deployment**
   - Serves content from gh-pages branch root directory
   - Available at: https://djensenius.github.io/search-backend/search_engine_backend/

## Manual Setup Required

To complete the setup, repository administrators need to:

1. **Enable GitHub Pages**:
   - Go to repository Settings → Pages
   - Under "Source", select "Deploy from a branch"  
   - Choose `gh-pages` branch and `/ (root)` folder
   - Save the configuration

2. **Verify Deployment**:
   - Push changes to main branch
   - Check Actions tab for workflow execution
   - Visit the GitHub Pages URL once deployment completes

## Branch Management

- **gh-pages branch**: Automatically managed by GitHub Actions
- **Manual changes**: Not recommended (will be overwritten)
- **Content updates**: Happen automatically on main branch pushes

## Troubleshooting

If documentation is not updating:

1. Check workflow permissions include `contents: write`
2. Verify gh-pages branch exists in repository
3. Confirm GitHub Pages is configured in repository settings
4. Review Actions tab for workflow failures

---

Created as part of Issue #13 implementation.
