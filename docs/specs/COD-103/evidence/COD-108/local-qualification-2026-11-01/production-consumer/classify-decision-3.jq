def cod108_syntax_check($classified): 
          .mode = $classified[0].mode | .reason_code = $classified[0].reason_code |
          .positive_metadata = (.mode == "lightweight"); null
