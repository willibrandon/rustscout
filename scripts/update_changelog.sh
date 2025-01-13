#!/bin/bash

# Get the new version (from tag)
NEW_VERSION=$1
if [ -z "$NEW_VERSION" ]; then
    echo "Error: Version argument required"
    exit 1
fi

# Get the date for the release
RELEASE_DATE=$(date +%Y-%m-%d)

# Get the previous git tag
PREVIOUS_TAG=$(git describe --tags --abbrev=0 HEAD^ 2>/dev/null || echo "")

# Get list of commits since last tag
if [ -z "$PREVIOUS_TAG" ]; then
    # If no previous tag, get all commits
    COMMITS=$(git log --pretty=format:"%s")
else
    # Get commits since last tag
    COMMITS=$(git log --pretty=format:"%s" $PREVIOUS_TAG..HEAD)
fi

# Initialize arrays for different types of changes
declare -a FEATURES=()
declare -a FIXES=()
declare -a DOCS=()
declare -a OTHERS=()

# Categorize commits
while IFS= read -r commit; do
    if [[ $commit == feat:* ]]; then
        FEATURES+=("${commit#feat: }")
    elif [[ $commit == fix:* ]]; then
        FIXES+=("${commit#fix: }")
    elif [[ $commit == docs:* ]]; then
        DOCS+=("${commit#docs: }")
    else
        OTHERS+=("$commit")
    fi
done <<< "$COMMITS"

# Create new changelog entry
CHANGELOG_ENTRY="## [$NEW_VERSION] - $RELEASE_DATE

### Added
"

# Add features
for feature in "${FEATURES[@]}"; do
    CHANGELOG_ENTRY+="- $feature
"
done

# Add fixes if any
if [ ${#FIXES[@]} -gt 0 ]; then
    CHANGELOG_ENTRY+="
### Fixed
"
    for fix in "${FIXES[@]}"; do
        CHANGELOG_ENTRY+="- $fix
"
    done
fi

# Add documentation changes if any
if [ ${#DOCS[@]} -gt 0 ]; then
    CHANGELOG_ENTRY+="
### Documentation
"
    for doc in "${DOCS[@]}"; do
        CHANGELOG_ENTRY+="- $doc
"
    done
fi

# Add other changes if any
if [ ${#OTHERS[@]} -gt 0 ]; then
    CHANGELOG_ENTRY+="
### Other
"
    for other in "${OTHERS[@]}"; do
        CHANGELOG_ENTRY+="- $other
"
    done
fi

# Add links section
CHANGELOG_ENTRY+="
[Unreleased]: https://github.com/willibrandon/rustscout/compare/$NEW_VERSION...HEAD
[$NEW_VERSION]: https://github.com/willibrandon/rustscout/releases/tag/$NEW_VERSION"

# Create or update CHANGELOG.md
if [ -f CHANGELOG.md ]; then
    # Create temp file
    TEMP_FILE=$(mktemp)
    echo "# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

$CHANGELOG_ENTRY" > "$TEMP_FILE"
    tail -n +2 CHANGELOG.md >> "$TEMP_FILE"
    mv "$TEMP_FILE" CHANGELOG.md
else
    echo "# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

$CHANGELOG_ENTRY" > CHANGELOG.md
fi 