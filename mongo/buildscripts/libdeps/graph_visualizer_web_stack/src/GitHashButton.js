import React from "react";
import { connect } from "react-redux";
import LoadingButton from "@material-ui/lab/LoadingButton";
import GitIcon from "@material-ui/icons/GitHub";
import { green, grey } from "@material-ui/core/colors";

import { getGraphFiles } from "./redux/store";
import { setLoading } from "./redux/loading";
import theme from "./theme";
import { selectGraphFile } from "./redux/graphFiles";
import { nodeInfo, setNodeInfos } from "./redux/nodeInfo";

const selectedStyle = {
  color: theme.palette.getContrastText(green[500]),
  backgroundColor: green[500],
  "&:hover": {
    backgroundColor: green[400],
  },
  "&:active": {
    backgroundColor: green[700],
  },
};

const unselectedStyle = {
  color: theme.palette.getContrastText(grey[100]),
  backgroundColor: grey[100],
  "&:hover": {
    backgroundColor: grey[200],
  },
  "&:active": {
    backgroundColor: grey[400],
  },
};

const GitHashButton = ({ loading, graphFiles, setLoading, selectGraphFile, setNodeInfos, text }) => {
  const [selected, setSelected] = React.useState(false);
  const [selfLoading, setSelfLoading] = React.useState(false);
  const [firstLoad, setFirstLoad] = React.useState(true);

  function handleClick() {
    const selectedGraphFiles = graphFiles.filter(
      (graphFile) => graphFile.selected == true
    );

    if (selectedGraphFiles.length > 0) {
      if (selectedGraphFiles[0]["git"] == text) {
        return;
      }
    }
    
    setSelfLoading(true);
    setLoading(true);
    selectGraphFile({
      hash: text,
      selected: true,
    });
  }

  React.useEffect(() => {
    const selectedGraphFile = graphFiles.filter(
      (graphFile) => graphFile.git == text
    );
    setSelected(selectedGraphFile[0].selected);

    if (firstLoad && graphFiles.length > 0) {
      if (graphFiles[0]["git"] == text) {
        handleClick();
      }
      setFirstLoad(false);
    }
  }, [graphFiles]);

  React.useEffect(() => {
    if (!loading) {
      setSelfLoading(false);
    }
  }, [loading]);

  return (
    <LoadingButton
      pending={selfLoading}
      pendingPosition="start"
      startIcon={<GitIcon />}
      variant="contained"
      style={selected ? selectedStyle : unselectedStyle}
      onClick={handleClick}
    >
      {text}
    </LoadingButton>
  );
};

export default connect(getGraphFiles, { setLoading, selectGraphFile, setNodeInfos })(GitHashButton);
