def cod108_syntax_check($id; $repository): 
  length == 1 and (.[0] | type == "object" and .repository.full_name == $repository and
  (.repository.id | tostring) == $id); null
