import "core-js/stable";
import "regenerator-runtime/runtime";

import { Client, Registry, proto } from "exocore";

import React from 'react';
import ReactDOM from 'react-dom';
import List from './list.js';

class App extends React.Component {
  constructor(props) {
    super(props);

    this.state = {exocore: null};

    Registry.registerMessage(proto.exocore.test.TestMessage, 'exocore.test.TestMessage');
    Registry.registerMessage(proto.exocore.test.TestMessage2, 'exocore.test.TestMessage2');

    console.log('Connecting...');
    Client.create("/dns4/localhost/tcp/3341/ws", (status) => {
      console.log('Status ' + status);
      this.setState({
        status: status,
      });
    }).then((client) => {
      this.setState({
        exocore: client,
      });
    });
  }

  render() {
    if (this.state.exocore && this.state.status === 'connected') {
      return (<div>
        <button onClick={this.disconnect.bind(this)}>Disconnect</button>

        <List exocore={this.state.exocore}/>
      </div>);
    } else {
      return this.renderLoading();
    }
  }

  renderLoading() {
    return <div>Connecting...</div>;
  }

  disconnect() {
    this.setState({exocore: null});
  }
}

ReactDOM.render(
  <App/>,
  document.getElementById('root')
);

