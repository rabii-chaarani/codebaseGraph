def cod108_syntax_check($base; $head): 
        length == 1 and (.[0] |
          type == "object" and .schema_version == 1 and
          (.mode == "full" or .mode == "lightweight") and
          (.reason_code | type == "string" and test("^[a-z0-9-]{1,64}$")) and
          .base_sha == $base and .head_sha == $head and
          (.mode != "lightweight" or .reason_code == "metadata-only-version-bump")); null
