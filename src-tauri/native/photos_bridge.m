#import <CommonCrypto/CommonDigest.h>
#import <Foundation/Foundation.h>
#import <Photos/Photos.h>

static const NSUInteger DSMaxChunkBytes = 8 * 1024 * 1024;
static const int64_t DSResourceTimeoutNanos = 30LL * NSEC_PER_SEC;
static const int64_t DSAuthorizationTimeoutNanos = 5LL * 60LL * NSEC_PER_SEC;

static char *DSJSON(id value) {
  NSData *data = [NSJSONSerialization dataWithJSONObject:value options:0 error:nil];
  NSString *text = data ? [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding] : @"{\"error\":\"photos-json-failed\"}";
  return strdup(text.UTF8String);
}

static NSString *DSStatus(PHAuthorizationStatus status) {
  switch (status) {
    case PHAuthorizationStatusAuthorized: return @"authorized";
    case PHAuthorizationStatusLimited: return @"limited";
    case PHAuthorizationStatusDenied: return @"denied";
    case PHAuthorizationStatusRestricted: return @"restricted";
    case PHAuthorizationStatusNotDetermined: return @"not-determined";
  }
  return @"unknown";
}

static NSString *DSHex(const unsigned char *bytes, NSUInteger length) {
  NSMutableString *hex = [NSMutableString stringWithCapacity:length * 2];
  for (NSUInteger index = 0; index < length; index++) [hex appendFormat:@"%02x", bytes[index]];
  return hex;
}

static NSString *DSMetadataFingerprint(PHAsset *asset, PHAssetResource *resource) {
  NSString *text = [NSString stringWithFormat:@"%@\n%ld\n%ld\n%.0f\n%.0f\n%@\n%@\n%ld",
                    asset.localIdentifier, (long)asset.pixelWidth, (long)asset.pixelHeight,
                    asset.creationDate.timeIntervalSince1970 * 1000,
                    asset.modificationDate.timeIntervalSince1970 * 1000,
                    resource.originalFilename ?: @"", resource.uniformTypeIdentifier ?: @"", (long)resource.type];
  NSData *data = [text dataUsingEncoding:NSUTF8StringEncoding];
  unsigned char digest[CC_SHA256_DIGEST_LENGTH];
  CC_SHA256(data.bytes, (CC_LONG)data.length, digest);
  return DSHex(digest, sizeof(digest));
}

static NSDictionary *DSReadResource(PHAsset *asset, uint64_t maxBytes) {
  NSArray<PHAssetResource *> *resources = [PHAssetResource assetResourcesForAsset:asset];
  if (resources.count != 1) return @{ @"state": @"unavailable", @"blocker": @"compound-photo-review-unavailable" };
  PHAssetResource *resource = resources.firstObject;
  if (!resource) return @{ @"state": @"unavailable", @"blocker": @"no-original-resource" };
  PHAssetResourceRequestOptions *options = [PHAssetResourceRequestOptions new];
  options.networkAccessAllowed = NO;
  dispatch_semaphore_t done = dispatch_semaphore_create(0);
  __block CC_SHA256_CTX context;
  CC_SHA256_Init(&context);
  __block uint64_t byteCount = 0;
  __block BOOL exceeded = NO;
  __block NSError *completionError = nil;
  PHAssetResourceDataRequestID requestID = [[PHAssetResourceManager defaultManager]
      requestDataForAssetResource:resource
      options:options
      dataReceivedHandler:^(NSData *data) {
        if (data.length > DSMaxChunkBytes || byteCount > maxBytes || data.length > maxBytes - MIN(byteCount, maxBytes)) {
          exceeded = YES;
          return;
        }
        byteCount += data.length;
        CC_SHA256_Update(&context, data.bytes, (CC_LONG)data.length);
      }
      completionHandler:^(NSError *error) {
        completionError = error;
        dispatch_semaphore_signal(done);
      }];
  if (dispatch_semaphore_wait(done, dispatch_time(DISPATCH_TIME_NOW, DSResourceTimeoutNanos)) != 0) {
    [[PHAssetResourceManager defaultManager] cancelDataRequest:requestID];
    dispatch_semaphore_wait(done, dispatch_time(DISPATCH_TIME_NOW, 5LL * NSEC_PER_SEC));
    return @{ @"state": @"unavailable", @"blocker": @"local-content-read-timed-out" };
  }
  if (completionError) {
    return @{ @"state": @"icloud-only-or-unavailable", @"blocker": @"download-original-in-photos" };
  }
  if (exceeded) return @{ @"state": @"unavailable", @"blocker": @"local-content-exceeds-review-limit" };
  unsigned char digest[CC_SHA256_DIGEST_LENGTH];
  CC_SHA256_Final(digest, &context);
  return @{ @"state": @"local-current", @"content_sha256": DSHex(digest, sizeof(digest)),
            @"encoded_bytes": @(byteCount), @"original_filename": resource.originalFilename ?: @"",
            @"uniform_type_identifier": resource.uniformTypeIdentifier ?: @"",
            @"resource_type": @((NSInteger)resource.type),
            @"metadata_fingerprint": DSMetadataFingerprint(asset, resource) };
}

static NSDictionary *DSAssetEvidence(PHAsset *asset, uint64_t maxBytes) {
  NSMutableDictionary *result = [@{
    @"local_identifier": asset.localIdentifier,
    @"width_pixels": @(asset.pixelWidth), @"height_pixels": @(asset.pixelHeight),
    @"pixel_count": @((uint64_t)asset.pixelWidth * (uint64_t)asset.pixelHeight),
    @"creation_ms": asset.creationDate ? @(llround(asset.creationDate.timeIntervalSince1970 * 1000)) : [NSNull null],
    @"modification_ms": asset.modificationDate ? @(llround(asset.modificationDate.timeIntervalSince1970 * 1000)) : [NSNull null]
  } mutableCopy];
  [result addEntriesFromDictionary:DSReadResource(asset, maxBytes)];
  return result;
}

static NSDictionary *DSInventory(NSUInteger maxAssets, uint64_t maxBytes, NSArray<NSString *> *identifiers) {
  PHAuthorizationStatus status = [PHPhotoLibrary authorizationStatusForAccessLevel:PHAccessLevelReadWrite];
  if (status != PHAuthorizationStatusAuthorized && status != PHAuthorizationStatusLimited) {
    return @{ @"authorization": DSStatus(status), @"evidence_complete": @NO, @"inventory_truncated": @NO,
              @"next_action": status == PHAuthorizationStatusNotDetermined ? @"connect-photos" : @"allow-photos-in-system-settings",
              @"assets": @[], @"exact_groups": @[], @"unavailable_count": @0 };
  }
  PHFetchResult<PHAsset *> *fetch;
  if (identifiers) {
    fetch = [PHAsset fetchAssetsWithLocalIdentifiers:identifiers options:nil];
  } else {
    PHFetchOptions *options = [PHFetchOptions new];
    options.predicate = [NSPredicate predicateWithFormat:@"mediaType == %d", PHAssetMediaTypeImage];
    fetch = [PHAsset fetchAssetsWithOptions:options];
  }
  NSMutableArray *assets = [NSMutableArray arrayWithCapacity:fetch.count];
  NSMutableDictionary<NSString *, NSMutableArray *> *byDigest = [NSMutableDictionary dictionary];
  __block NSUInteger unavailable = 0;
  NSUInteger reviewCount = MIN(fetch.count, maxAssets);
  for (NSUInteger index = 0; index < reviewCount; index++) {
    PHAsset *asset = [fetch objectAtIndex:index];
    (void)index;
    NSDictionary *evidence = DSAssetEvidence(asset, maxBytes);
    [assets addObject:evidence];
    NSString *digest = evidence[@"content_sha256"];
    if (digest) {
      if (!byDigest[digest]) byDigest[digest] = [NSMutableArray array];
      [byDigest[digest] addObject:evidence];
    } else unavailable++;
  }
  NSMutableArray *groups = [NSMutableArray array];
  for (NSString *digest in [[byDigest allKeys] sortedArrayUsingSelector:@selector(compare:)]) {
    NSArray *members = byDigest[digest];
    if (members.count > 1) [groups addObject:@{ @"content_sha256": digest, @"members": members,
      @"keeper_required": @YES, @"automatic_delete_allowed": @NO }];
  }
  NSDictionary *fingerprintSource = @{ @"assets": assets, @"unavailable_count": @(unavailable) };
  NSData *canonical = [NSJSONSerialization dataWithJSONObject:fingerprintSource options:NSJSONWritingSortedKeys error:nil];
  unsigned char digest[CC_SHA256_DIGEST_LENGTH]; CC_SHA256(canonical.bytes, (CC_LONG)canonical.length, digest);
  BOOL truncated = fetch.count > reviewCount;
  return @{ @"authorization": DSStatus(status), @"observed_at_ms": @((uint64_t)(NSDate.date.timeIntervalSince1970 * 1000)),
            @"inventory_fingerprint": DSHex(digest, sizeof(digest)), @"evidence_complete": @(!truncated && unavailable == 0),
            @"inventory_truncated": @(truncated),
            @"next_action": truncated ? @"reduce-photos-library-review-scope" : (unavailable ? @"download-originals-in-photos" : (groups.count ? @"choose-one-photo-to-keep-per-group" : @"no-exact-duplicates-found")),
            @"assets": assets, @"exact_groups": groups, @"unavailable_count": @(unavailable),
            @"near_duplicate_evidence": @"unavailable-without-measured-content-equivalence" };
}

char *ds_photos_authorization_status(void) {
  return DSJSON(@{ @"authorization": DSStatus([PHPhotoLibrary authorizationStatusForAccessLevel:PHAccessLevelReadWrite]) });
}

char *ds_photos_request_authorization(void) {
  dispatch_semaphore_t done = dispatch_semaphore_create(0);
  __block PHAuthorizationStatus result = PHAuthorizationStatusNotDetermined;
  [PHPhotoLibrary requestAuthorizationForAccessLevel:PHAccessLevelReadWrite handler:^(PHAuthorizationStatus status) {
    result = status; dispatch_semaphore_signal(done);
  }];
  if (dispatch_semaphore_wait(done, dispatch_time(DISPATCH_TIME_NOW, DSAuthorizationTimeoutNanos)) != 0)
    return DSJSON(@{ @"authorization": @"timed-out" });
  return DSJSON(@{ @"authorization": DSStatus(result) });
}

char *ds_photos_inventory(uint32_t maxAssets, uint64_t maxBytes) {
  return DSJSON(DSInventory(MIN((NSUInteger)maxAssets, 10000), MIN(maxBytes, 536870912ULL), nil));
}

char *ds_photos_delete(const char *requestJSON) {
  NSData *data = [[NSData alloc] initWithBytes:requestJSON length:strlen(requestJSON)];
  NSDictionary *request = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
  NSArray<NSString *> *identifiers = request[@"delete_identifiers"];
  NSDictionary<NSString *, NSString *> *expected = request[@"expected_metadata_fingerprints"];
  NSDictionary<NSString *, NSString *> *expectedContent = request[@"expected_content_sha256"];
  if (![identifiers isKindOfClass:NSArray.class] || !identifiers.count || ![expected isKindOfClass:NSDictionary.class] ||
      ![expectedContent isKindOfClass:NSDictionary.class] || expected.count != expectedContent.count)
    return DSJSON(@{ @"error": @"photos-delete-request-invalid" });
  NSArray<NSString *> *reviewedIdentifiers = expected.allKeys;
  NSDictionary *fresh = DSInventory(reviewedIdentifiers.count, [request[@"max_resource_bytes"] unsignedLongLongValue], reviewedIdentifiers);
  NSArray *freshAssets = fresh[@"assets"];
  if (freshAssets.count != reviewedIdentifiers.count) return DSJSON(@{ @"error": @"photos-library-changed-review-again" });
  for (NSDictionary *asset in freshAssets) {
    NSString *identifier = asset[@"local_identifier"];
    if (![expectedContent[identifier] isEqual:asset[@"content_sha256"]] ||
        ![expected[identifier] isEqual:asset[@"metadata_fingerprint"]])
      return DSJSON(@{ @"error": @"photos-library-changed-review-again" });
  }
  PHFetchResult<PHAsset *> *fetch = [PHAsset fetchAssetsWithLocalIdentifiers:identifiers options:nil];
  NSError *error = nil;
  BOOL success = [[PHPhotoLibrary sharedPhotoLibrary] performChangesAndWait:^{ [PHAssetChangeRequest deleteAssets:fetch]; } error:&error];
  if (!success) return DSJSON(@{ @"error": @"photos-system-delete-not-completed", @"system_message": error.localizedDescription ?: @"" });
  return DSJSON(@{ @"deleted_identifiers": identifiers, @"system_confirmation_completed": @YES });
}
