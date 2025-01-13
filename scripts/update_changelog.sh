#!/bin/bash

# Get the new version
NEW_VERSION=$1

if [ -z "$NEW_VERSION" ]; then
    echo "Usage: $0 <new-version>"
    echo "Example: $0 v0.2.0"
    exit 1
fi

# Get the current date
DATE=$(date +%Y-%m-%d)

# Get the previous tag
PREV_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")

# Generate commit list since last tag
if [ -z "$PREV_TAG" ]; then
    COMMITS=$(git log --pretty=format:"* %s" --no-merges)
else
    COMMITS=$(git log --pretty=format:"* %s" --no-merges $PREV_TAG..HEAD)
fi

# Categorize commits
FEATURES=$(echo "$COMMITS" | grep -i "^* feat" || true)
FIXES=$(echo "$COMMITS" | grep -i "^* fix" || true)
DOCS=$(echo "$COMMITS" | grep -i "^* docs" || true)
OTHERS=$(echo "$COMMITS" | grep -v -i "^* feat\|^* fix\|^* docs" || true)

# Create new changelog entry
ENTRY="## [$NEW_VERSION] - $DATE

### Added
$FEATURES

### Fixed
$FIXES

### Documentation
$DOCS

### Other Changes
$OTHERS
"

# Create temporary file
TEMP_FILE=$(mktemp)

# If CHANGELOG.md exists, insert after first line
if [ -f CHANGELOG.md ]; then
    head -n 1 CHANGELOG.md > "$TEMP_FILE"
    echo -e "\n$ENTRY" >> "$TEMP_FILE"
    tail -n +2 CHANGELOG.md >> "$TEMP_FILE"
else
    # Create new CHANGELOG.md
    echo "# Changelog" > "$TEMP_FILE"
    echo -e "\n$ENTRY" >> "$TEMP_FILE"
    echo -e "\n[Unreleased]: https://github.com/willibrandon/rustscout/compare/$NEW_VERSION...HEAD" >> "$TEMP_FILE"
    echo "[$NEW_VERSION]: https://github.com/willibrandon/rustscout/releases/tag/$NEW_VERSION" >> "$TEMP_FILE"
fi

# Replace old changelog with new one
mv "$TEMP_FILE" CHANGELOG.md 