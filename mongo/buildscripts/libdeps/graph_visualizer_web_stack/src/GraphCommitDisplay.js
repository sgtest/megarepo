import React from "react";
import ScrollContainer from "react-indiana-drag-scroll";
import { connect } from "react-redux";
import Table from "@material-ui/core/Table";
import TableBody from "@material-ui/core/TableBody";
import TableCell from "@material-ui/core/TableCell";
import Paper from "@material-ui/core/Paper";
import TableRow from "@material-ui/core/TableRow";
import List from "@material-ui/core/List";
import ListItem from "@material-ui/core/ListItem";
import TextField from "@material-ui/core/TextField";

import { getGraphFiles } from "./redux/store";
import { setGraphFiles } from "./redux/graphFiles";

import GitHashButton from "./GitHashButton";

const { REACT_APP_API_URL } = process.env;

const flexContainer = {
  display: "flex",
  flexDirection: "row",
  padding: 0,
  width: "50%",
  height: "50%",
};

const textFields = [
  "Scroll to commit",
  "Commit Range Begin",
  "Commit Range End",
];

const GraphCommitDisplay = ({ graphFiles, setGraphFiles }) => {
  React.useEffect(() => {
    fetch(REACT_APP_API_URL + "/api/graphs")
      .then((res) => res.json())
      .then((data) => {
        setGraphFiles(data.graph_files);
      })
      .catch((err) => {
        /* eslint-disable no-console */
        console.log("Error Reading data " + err);
      });
    }, []);

  return (
    <Paper style={{ height: "100%", width: "100%" }}>
      <List style={flexContainer}>
        {textFields.map((text) => (
          <ListItem key={text}>
            <TextField size="small" label={text} />
          </ListItem>
        ))}
      </List>
      <ScrollContainer
        vertical={false}
        style={{ height: "50%" }}
        className="scroll-container"
        hideScrollbars={true}
      >
        <Table style={{ height: "100%" }}>
          <TableBody>
            <TableRow>
              {graphFiles.map((file) => (
                <TableCell key={file.id}>
                  <GitHashButton text={file.git} />
                </TableCell>
              ))}
            </TableRow>
          </TableBody>
        </Table>
      </ScrollContainer>
    </Paper>
  );
};

export default connect(getGraphFiles, { setGraphFiles })(GraphCommitDisplay);
